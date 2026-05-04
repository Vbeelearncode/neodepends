//! Python dependency enhancement.
//!
//! Stack graphs resolve *where* a name is defined but cannot classify *why* it is referenced.
//! Every resolved reference comes out as a generic `Use` edge; `classify_stackgraph_dep` in
//! `stackgraphs.rs` handles Import/Extend/Call/Create at resolution time using AST context.
//! Several patterns still slip through:
//!
//! - `self.field` accesses on inherited fields — stack graphs follow name resolution into the
//!   base class file but do not know the field belongs to an inheritance relationship.
//!   Example: `Child.method` accesses `self.val` defined on `Base`; stack graphs emit a `Use`
//!   to `val` with no inheritance context.
//!
//! - Method calls via typed parameters — `param.method()` where `param: SomeClass` resolves
//!   only if stack graphs can statically trace the type, which requires full type inference.
//!   Example: `def process(self, ticket: Ticket): ticket.validate()` — stack graphs miss this.
//!
//! - Abstract method / override relationships — whether a method implements an abstract one
//!   requires cross-file inheritance traversal that stack graphs do not perform.
//!
//! - `@dataclass` field type annotations — `inventory: List[Station]` creates a structural
//!   dependency from the field to `Station` that stack graphs treat as a plain name reference
//!   without connecting it to the field entity.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::anyhow;
use anyhow::Result;
use tree_sitter::Language;
use tree_sitter::Parser;
use tree_sitter::Query;
use tree_sitter::QueryCursor;

use crate::core::DepKind;
use crate::core::Entity;
use crate::core::EntityDep;
use crate::core::EntityId;
use crate::core::EntityKind;
use crate::core::Position;
use crate::enhancement::DepEnhancer;
use crate::enhancement::entity_index::dep_at_position;
use crate::enhancement::entity_index::dep_at_row;
use crate::enhancement::entity_index::dedup_edges;
use crate::enhancement::entity_index::EntityIndex;
use crate::enhancement::returns::AssignmentFact;
use crate::enhancement::returns::FieldFact;
use crate::enhancement::returns::ReceiverCallCandidate;
use crate::enhancement::returns::ReceiverCallFact;
use crate::enhancement::returns::ReturnExpr;
use crate::enhancement::returns::ReturnFact;
use crate::enhancement::returns::ReturnFacts;
use crate::enhancement::returns::ReturnSolver;
use crate::enhancement::returns::ReturnSolverOptions;
use crate::enhancement::returns::ReturnSolverResult;
use crate::enhancement::returns::TypeConfidence;
use crate::enhancement::returns::TypeEvidence;
use crate::enhancement::returns::TypeEvidenceSource;

#[derive(Debug, Clone)]
struct FieldParamAssignment {
    callable_id: EntityId,
    class_id: EntityId,
    field_name: String,
    param_name: String,
}

struct PythonReturnInference {
    facts: ReturnFacts,
    param_names: HashMap<EntityId, Vec<String>>,
    field_assignments: HashMap<EntityId, Vec<FieldParamAssignment>>,
}

struct FieldFactCallContext<'tree> {
    scope_id: EntityId,
    function: tree_sitter::Node<'tree>,
    arguments: Vec<(usize, String)>,
}

#[derive(Default)]
struct ReturnCaptures<'a> {
    return_var: Option<&'a str>,
    return_byte: usize,
    return_self_field: Option<&'a str>,
    return_self_field_byte: usize,
    return_call: Option<&'a str>,
    assign_var: Option<&'a str>,
    assign_byte: usize,
    assign_call: Option<&'a str>,
    assign_attr_var: Option<&'a str>,
    assign_attr_byte: usize,
    assign_attr_recv: Option<&'a str>,
    assign_attr_method: Option<&'a str>,
}

#[derive(Default)]
struct ReceiverCaptures<'a> {
    receiver_byte: Option<usize>,
    receiver_row: Option<usize>,
    receiver_col: Option<usize>,
    receiver_name: Option<&'a str>,
    method_name: Option<&'a str>,
    field_byte: Option<usize>,
    field_row: Option<usize>,
    field_col: Option<usize>,
    field_name: Option<&'a str>,
    field_method_name: Option<&'a str>,
}

#[derive(Debug)]
pub struct PythonQueryEnhancer {
    language: Language,
    query: Arc<Query>,
    capture_names: Vec<String>,
}

impl PythonQueryEnhancer {
    pub fn new(language: Language, query_str: &str) -> anyhow::Result<Self> {
        let query = Query::new(language, query_str)?;
        let capture_names = query.capture_names().iter().map(|s| s.to_string()).collect();
        Ok(Self { language, query: Arc::new(query), capture_names })
    }

    fn parse(&self, content: &str) -> Option<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser.set_language(self.language).ok()?;
        parser.parse(content, None)
    }

    fn index_base_classes_from_ast(&self, filename: &str, content: &str, index: &mut EntityIndex) {
        let Some(tree) = self.parse(content) else { return };
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            for cap in qmatch.captures {
                let capture_name = &self.capture_names[cap.index as usize];
                if capture_name != "extend.base" { continue; }
                let Ok(base_name) = cap.node.utf8_text(bytes) else { continue };
                let Some(subclass_id) = index.owner_at(filename, cap.node.start_byte()) else { continue };
                let Some(content_id) = index.content_id_of_file(filename) else { continue };
                if let Some(base_id) = index.resolve_class(base_name, content_id) {
                    if subclass_id != base_id {
                        index.add_base_class(subclass_id, base_id);
                    }
                }
            }
        }
    }

    fn index_var_types_from_ast(&self, filename: &str, content: &str, index: &mut EntityIndex) {
        let Some(tree) = self.parse(content) else { return };
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            let mut param_name: Option<&str> = None;
            let mut param_byte: usize = 0;
            let mut param_type_name: Option<&str> = None;
            let mut assigned_var: Option<&str> = None;
            let mut assign_byte: usize = 0;
            let mut assigned_class_name: Option<&str> = None;

            for cap in qmatch.captures {
                let capture_name = &self.capture_names[cap.index as usize];
                match capture_name.as_str() {
                    "param.name" => { param_name = cap.node.utf8_text(bytes).ok(); param_byte = cap.node.start_byte(); }
                    "param.type" => { param_type_name = cap.node.utf8_text(bytes).ok(); }
                    "assign.var" => { assigned_var = cap.node.utf8_text(bytes).ok(); assign_byte = cap.node.start_byte(); }
                    "assign.class" => { assigned_class_name = cap.node.utf8_text(bytes).ok(); }
                    _ => {}
                }
            }

            if let (Some(var), Some(type_name)) = (param_name, param_type_name) {
                if !matches!(var, "self" | "cls") {
                    let content_id = index.content_id_of_file(filename);
                    let method_id = index.owner_at(filename, param_byte);
                    if let (Some(content_id), Some(method_id)) = (content_id, method_id) {
                        if let Some(class_id) = index.resolve_class(type_name, content_id) {
                            index.add_var_type(method_id, var.to_string(), class_id);
                        }
                    }
                }
            }

            if let (Some(var), Some(class_name)) = (assigned_var, assigned_class_name) {
                let content_id = index.content_id_of_file(filename);
                let method_id = index.owner_at(filename, assign_byte);
                if let (Some(content_id), Some(method_id)) = (content_id, method_id) {
                    if let Some(class_id) = index.resolve_class(class_name, content_id) {
                        index.add_var_type(method_id, var.to_string(), class_id);
                    }
                }
            }
        }
    }

    fn emit_typed_deps_for_file(
        &self,
        filename: &str,
        content: &str,
        all_sources: &HashMap<String, String>,
        index: &EntityIndex,
        abstract_method_ids: &mut HashSet<EntityId>,
    ) -> Result<Vec<EntityDep>> {
        let tree = self.parse(content)
            .ok_or_else(|| anyhow!("tree-sitter parse failed for {}", filename))?;
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();
        let mut new_deps: Vec<EntityDep> = Vec::new();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            let mut recv_byte: Option<usize> = None;
            let mut recv_row: Option<usize> = None;
            let mut recv_col: Option<usize> = None;
            let mut recv_text: Option<&str> = None;
            let mut recv_method_name: Option<&str> = None;
            let mut cap_kind: Option<&str> = None;
            let mut cap_byte: usize = 0;
            let mut cap_row: usize = 0;
            let mut cap_col: usize = 0;
            let mut cap_text: Option<&str> = None;

            for cap in qmatch.captures {
                let name = &self.capture_names[cap.index as usize];
                if name.starts_with('_') { continue; }
                let node = cap.node;
                match name.as_str() {
                    "call.class_recv" => {
                        recv_byte = Some(node.start_byte());
                        let pt = node.start_position();
                        recv_row = Some(pt.row);
                        recv_col = Some(pt.column);
                        recv_text = node.utf8_text(bytes).ok();
                    }
                    "call.class_method" => {
                        recv_method_name = node.utf8_text(bytes).ok();
                    }
                    _ => {
                        cap_kind = Some(name.as_str());
                        cap_byte = node.start_byte();
                        let pt = node.start_position();
                        cap_row = pt.row;
                        cap_col = pt.column;
                        cap_text = node.utf8_text(bytes).ok();
                    }
                }
            }

            if let (Some(rb), Some(rr), Some(rc), Some(rt), Some(method_name)) =
                (recv_byte, recv_row, recv_col, recv_text, recv_method_name)
            {
                if !matches!(rt, "self" | "cls" | "super") {
                    new_deps.extend(self.emit_class_receiver_call(filename, rb, rr, rc, rt, method_name, index));
                }
                continue;
            }

            let (Some(kind), Some(text)) = (cap_kind, cap_text) else { continue };

            match kind {
                "use.self_field" =>
                    new_deps.extend(self.emit_self_field_use(filename, cap_byte, cap_row, cap_col, text, index)),
                "call.self_method" =>
                    new_deps.extend(self.emit_self_method_call(filename, cap_byte, cap_row, cap_col, text, index)),
                "call.super_method" =>
                    new_deps.extend(self.emit_super_method_call(filename, cap_byte, cap_row, cap_col, text, index)),
                "extend.base" =>
                    new_deps.extend(self.emit_extend_dep(filename, cap_byte, cap_row, cap_col, text, index)),
                "import.module" | "import_from.module" =>
                    new_deps.extend(self.emit_import_dep(filename, cap_byte, cap_row, cap_col, text, all_sources, index)),
                "create.class" =>
                    new_deps.extend(self.emit_create_dep(filename, cap_byte, cap_row, cap_col, text, index)),
                "abstract.method" => {
                    if let Some(entity_id) = index.owner_at(filename, cap_byte) {
                        if index.entity(entity_id).map_or(false, |e| e.kind == EntityKind::Method) {
                            abstract_method_ids.insert(entity_id);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(new_deps)
    }

    fn extract_return_facts_from_ast(&self, filename: &str, content: &str, index: &EntityIndex) -> ReturnFacts {
        let Some(tree) = self.parse(content) else { return ReturnFacts::default() };
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();
        let mut facts = ReturnFacts::default();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            let Some(content_id) = index.content_id_of_file(filename) else { continue };
            let captures = self.return_captures(&qmatch, bytes);
            self.push_return_var_fact(filename, &captures, index, &mut facts);
            self.push_return_field_fact(filename, &captures, index, &mut facts);
            self.push_return_call_fact(filename, content_id, &captures, index, &mut facts);
            self.push_assignment_call_fact(filename, content_id, &captures, index, &mut facts);
            self.push_assignment_method_fact(filename, content_id, &captures, index, &mut facts);
        }

        facts
    }

    fn return_captures<'a, 'tree>(
        &self,
        qmatch: &tree_sitter::QueryMatch<'a, 'tree>,
        bytes: &'tree [u8],
    ) -> ReturnCaptures<'tree> {
        let mut captures = ReturnCaptures::default();
        for cap in qmatch.captures {
            let capture_name = &self.capture_names[cap.index as usize];
            self.set_return_capture(capture_name, cap.node, bytes, &mut captures);
        }
        captures
    }

    fn set_return_capture<'tree>(
        &self,
        capture_name: &str,
        node: tree_sitter::Node<'tree>,
        bytes: &'tree [u8],
        captures: &mut ReturnCaptures<'tree>,
    ) {
        let text = node.utf8_text(bytes).ok();
        match capture_name {
            "return.var" => { captures.return_var = text; captures.return_byte = node.start_byte(); }
            "return.self_field" => { captures.return_self_field = text; captures.return_self_field_byte = node.start_byte(); }
            "return.call" => { captures.return_call = text; captures.return_byte = node.start_byte(); }
            "return_assign.var" => { captures.assign_var = text; captures.assign_byte = node.start_byte(); }
            "return_assign.call" => captures.assign_call = text,
            "return_assign_attr.var" => { captures.assign_attr_var = text; captures.assign_attr_byte = node.start_byte(); }
            "return_assign_attr.recv" => captures.assign_attr_recv = text,
            "return_assign_attr.method" => captures.assign_attr_method = text,
            _ => {}
        }
    }

    fn push_return_var_fact(&self, filename: &str, captures: &ReturnCaptures, index: &EntityIndex, facts: &mut ReturnFacts) {
        let Some(var_name) = captures.return_var else { return };
        let Some(callable_id) = index.owner_at(filename, captures.return_byte) else { return };
        facts.returns.push(ReturnFact {
            callable_id,
            expr: ReturnExpr::Variable(var_name.to_string()),
            evidence: TypeEvidence::high(TypeEvidenceSource::ReturnVariable),
        });
    }

    fn push_return_field_fact(&self, filename: &str, captures: &ReturnCaptures, index: &EntityIndex, facts: &mut ReturnFacts) {
        let Some(field_name) = captures.return_self_field else { return };
        let Some(callable_id) = index.owner_at(filename, captures.return_self_field_byte) else { return };
        let Some(class_id) = index.enclosing_class_of(callable_id) else { return };
        facts.returns.push(ReturnFact {
            callable_id,
            expr: ReturnExpr::Field { class_id, field_name: field_name.to_string() },
            evidence: TypeEvidence::high(TypeEvidenceSource::ReturnField),
        });
    }

    fn push_return_call_fact(
        &self,
        filename: &str,
        content_id: crate::core::ContentId,
        captures: &ReturnCaptures,
        index: &EntityIndex,
        facts: &mut ReturnFacts,
    ) {
        let Some(call_name) = captures.return_call else { return };
        let Some(callable_id) = index.owner_at(filename, captures.return_byte) else { return };
        let Some(expr) = self.type_expr_for_call_name(call_name, content_id, callable_id, index) else { return };
        facts.returns.push(ReturnFact {
            callable_id,
            expr,
            evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
        });
    }

    fn push_assignment_call_fact(
        &self,
        filename: &str,
        content_id: crate::core::ContentId,
        captures: &ReturnCaptures,
        index: &EntityIndex,
        facts: &mut ReturnFacts,
    ) {
        let (Some(var_name), Some(call_name)) = (captures.assign_var, captures.assign_call) else { return };
        let Some(scope_id) = index.owner_at(filename, captures.assign_byte) else { return };
        let Some(expr) = self.type_expr_for_call_name(call_name, content_id, scope_id, index) else { return };
        facts.assignments.push(AssignmentFact {
            scope_id,
            variable: var_name.to_string(),
            expr,
            evidence: TypeEvidence::high(TypeEvidenceSource::AssignmentConstructor),
        });
    }

    fn push_assignment_method_fact(
        &self,
        filename: &str,
        content_id: crate::core::ContentId,
        captures: &ReturnCaptures,
        index: &EntityIndex,
        facts: &mut ReturnFacts,
    ) {
        let (Some(var_name), Some(receiver_name), Some(method_name)) =
            (captures.assign_attr_var, captures.assign_attr_recv, captures.assign_attr_method)
        else { return };
        let Some(scope_id) = index.owner_at(filename, captures.assign_attr_byte) else { return };
        let expr = self.method_call_expr(receiver_name, method_name, content_id, index);
        facts.assignments.push(AssignmentFact {
            scope_id,
            variable: var_name.to_string(),
            expr,
            evidence: TypeEvidence::high(TypeEvidenceSource::ReturnCall),
        });
    }

    fn type_expr_for_call_name(
        &self,
        call_name: &str,
        content_id: crate::core::ContentId,
        callable_id: EntityId,
        index: &EntityIndex,
    ) -> Option<ReturnExpr> {
        if call_name == "cls" {
            return index.enclosing_class_of(callable_id).map(ReturnExpr::Type);
        }
        index.resolve_class(call_name, content_id)
            .map(ReturnExpr::Type)
            .or_else(|| index.resolve_callable(call_name, content_id).map(ReturnExpr::Call))
    }

    fn method_call_expr(
        &self,
        receiver_name: &str,
        method_name: &str,
        content_id: crate::core::ContentId,
        index: &EntityIndex,
    ) -> ReturnExpr {
        if let Some(class_id) = index.resolve_class(receiver_name, content_id) {
            if let Some(callee_id) = index.resolve_method(class_id, method_name) {
                return ReturnExpr::Call(callee_id);
            }
        }
        ReturnExpr::MethodCall {
            receiver: Box::new(ReturnExpr::Variable(receiver_name.to_string())),
            method_name: method_name.to_string(),
        }
    }

    fn extract_param_names_from_ast(&self, filename: &str, content: &str, index: &EntityIndex) -> HashMap<EntityId, Vec<String>> {
        let Some(tree) = self.parse(content) else { return HashMap::new() };
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();
        let mut params: HashMap<EntityId, Vec<(usize, String)>> = HashMap::new();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            for cap in qmatch.captures {
                let capture_name = &self.capture_names[cap.index as usize];
                if capture_name != "function.param" { continue; }
                let Ok(param_name) = cap.node.utf8_text(bytes) else { continue };
                let Some(callable_id) = index.owner_at(filename, cap.node.start_byte()) else { continue };
                params
                    .entry(callable_id)
                    .or_default()
                    .push((cap.node.start_byte(), param_name.to_string()));
            }
        }

        params
            .into_iter()
            .map(|(callable_id, mut names)| {
                names.sort_by_key(|(byte, _)| *byte);
                names.dedup_by(|a, b| a.1 == b.1);
                (callable_id, names.into_iter().map(|(_, name)| name).collect())
            })
            .collect()
    }

    fn extract_field_param_assignments_from_ast(&self, filename: &str, content: &str, index: &EntityIndex) -> Vec<FieldParamAssignment> {
        let Some(tree) = self.parse(content) else { return vec![] };
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();
        let mut result = Vec::new();

        for qmatch in cursor.matches(&self.query, tree.root_node(), bytes) {
            if let Some(assignment) = self.field_param_assignment(filename, &qmatch, bytes, index) {
                result.push(assignment);
            }
        }

        result
    }

    fn field_param_assignment<'a, 'tree>(
        &self,
        filename: &str,
        qmatch: &tree_sitter::QueryMatch<'a, 'tree>,
        bytes: &'tree [u8],
        index: &EntityIndex,
    ) -> Option<FieldParamAssignment> {
        let (byte, field_name, param_name) = self.field_assignment_parts(qmatch, bytes)?;
        let callable_id = callable_owner_at(filename, byte, index)?;
        Some(FieldParamAssignment {
            callable_id,
            class_id: index.enclosing_class_of(callable_id)?,
            field_name: field_name.to_string(),
            param_name: param_name.to_string(),
        })
    }

    fn field_assignment_parts<'a, 'tree>(
        &self,
        qmatch: &tree_sitter::QueryMatch<'a, 'tree>,
        bytes: &'tree [u8],
    ) -> Option<(usize, &'tree str, &'tree str)> {
        let mut field_name = None;
        let mut param_name = None;
        let mut byte = 0;
        for cap in qmatch.captures {
            match self.capture_names[cap.index as usize].as_str() {
                "field_assign.field" => { field_name = cap.node.utf8_text(bytes).ok(); byte = cap.node.start_byte(); }
                "field_assign.param" => param_name = cap.node.utf8_text(bytes).ok(),
                _ => {}
            }
        }
        Some((byte, field_name?, param_name?))
    }

    fn derive_field_facts_from_call_sites(
        &self,
        filename: &str,
        content: &str,
        index: &EntityIndex,
        return_result: &ReturnSolverResult,
        param_names: &HashMap<EntityId, Vec<String>>,
        field_assignments: &HashMap<EntityId, Vec<FieldParamAssignment>>,
    ) -> Vec<FieldFact> {
        let Some(tree) = self.parse(content) else { return vec![] };
        let bytes = content.as_bytes();
        let Some(content_id) = index.content_id_of_file(filename) else { return vec![] };
        let mut result = Vec::new();

        walk_tree(tree.root_node(), &mut |node| {
            result.extend(self.derive_field_facts_from_call(
                filename,
                node,
                bytes,
                content_id,
                index,
                return_result,
                param_names,
                field_assignments,
            ));
        });

        result
    }

    fn derive_field_facts_from_call(
        &self,
        filename: &str,
        node: tree_sitter::Node,
        bytes: &[u8],
        content_id: crate::core::ContentId,
        index: &EntityIndex,
        return_result: &ReturnSolverResult,
        param_names: &HashMap<EntityId, Vec<String>>,
        field_assignments: &HashMap<EntityId, Vec<FieldParamAssignment>>,
    ) -> Vec<FieldFact> {
        let Some(call) = field_fact_call_context(filename, node, bytes, index) else { return vec![] };
        self.resolve_call_targets(call.function, bytes, content_id, call.scope_id, index, return_result)
            .into_iter()
            .flat_map(|(target_callable_id, _)| {
                field_facts_for_target(target_callable_id, call.scope_id, &call.arguments, return_result, param_names, field_assignments)
            })
            .collect()
    }

    fn resolve_call_targets(
        &self,
        function: tree_sitter::Node,
        bytes: &[u8],
        content_id: crate::core::ContentId,
        scope_id: EntityId,
        index: &EntityIndex,
        return_result: &ReturnSolverResult,
    ) -> Vec<(EntityId, EntityId)> {
        match function.kind() {
            "identifier" => constructor_call_target(function, bytes, content_id, index),
            "attribute" => method_call_targets(function, bytes, content_id, scope_id, index, return_result),
            _ => vec![],
        }
    }

    fn emit_return_inferred_receiver_calls_for_file(
        &self,
        filename: &str,
        content: &str,
        index: &EntityIndex,
        return_result: &ReturnSolverResult,
    ) -> Vec<EntityDep> {
        let Some(tree) = self.parse(content) else { return vec![] };
        let bytes = content.as_bytes();
        let mut receiver_calls = self.query_receiver_calls(filename, content, tree.root_node(), index);
        self_field_receiver_calls(filename, tree.root_node(), bytes, index, &mut receiver_calls);

        return_result
            .resolve_receiver_calls(&receiver_calls, index, TypeConfidence::High)
            .into_iter()
            .filter_map(|candidate| inferred_receiver_dep(candidate, index))
            .collect()
    }

    fn query_receiver_calls(
        &self,
        filename: &str,
        content: &str,
        root_node: tree_sitter::Node,
        index: &EntityIndex,
    ) -> Vec<ReceiverCallFact> {
        let mut cursor = QueryCursor::new();
        let bytes = content.as_bytes();
        let mut receiver_calls = Vec::new();

        for qmatch in cursor.matches(&self.query, root_node, bytes) {
            let captures = self.receiver_captures(&qmatch, bytes);
            if let Some(call) = receiver_call_from_variable(filename, &captures, index) {
                receiver_calls.push(call);
            }
            if let Some(call) = receiver_call_from_self_field(filename, &captures, index) {
                receiver_calls.push(call);
            }
        }

        receiver_calls
    }

    fn receiver_captures<'a, 'tree>(
        &self,
        qmatch: &tree_sitter::QueryMatch<'a, 'tree>,
        bytes: &'tree [u8],
    ) -> ReceiverCaptures<'tree> {
        let mut captures = ReceiverCaptures::default();
        for cap in qmatch.captures {
            let capture_name = &self.capture_names[cap.index as usize];
            set_receiver_capture(capture_name, cap.node, bytes, &mut captures);
        }
        captures
    }

    fn emit_self_field_use(
        &self, filename: &str, byte: usize, row: usize, col: usize, field_name: &str, index: &EntityIndex,
    ) -> Option<EntityDep> {
        let innermost = index.owner_at(filename, byte)?;
        let src_id = if index.entity(innermost).map_or(false, |e| e.kind == EntityKind::Field) {
            index.enclosing_callable_of(innermost)?
        } else {
            innermost
        };
        let class_id = index.enclosing_class_of(src_id)?;
        let tgt_id = index.resolve_field(class_id, field_name)?;
        (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Use, byte, row, col, index.commit_id_of_entity(src_id)))
    }

    fn emit_self_method_call(
        &self, filename: &str, byte: usize, row: usize, col: usize, method_name: &str, index: &EntityIndex,
    ) -> Option<EntityDep> {
        let src_id = index.owner_at(filename, byte)?;
        let class_id = index.enclosing_class_of(src_id)?;
        let tgt_id = index.resolve_method(class_id, method_name)?;
        (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Call, byte, row, col, index.commit_id_of_entity(src_id)))
    }

    fn emit_super_method_call(
        &self, filename: &str, byte: usize, row: usize, col: usize, method_name: &str, index: &EntityIndex,
    ) -> Vec<EntityDep> {
        let Some(src_id) = index.owner_at(filename, byte) else { return vec![] };
        let Some(class_id) = index.enclosing_class_of(src_id) else { return vec![] };
        let Some(bases) = index.bases_of(class_id) else { return vec![] };
        let commit_id = index.commit_id_of_entity(src_id);
        bases.iter()
            .filter_map(|&base_id| {
                let tgt_id = index.resolve_method(base_id, method_name)?;
                (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Call, byte, row, col, commit_id))
            })
            .collect()
    }

    fn emit_class_receiver_call(
        &self,
        filename: &str,
        byte: usize, row: usize, col: usize,
        receiver_name: &str,
        method_name: &str,
        index: &EntityIndex,
    ) -> Vec<EntityDep> {
        let Some(src_id) = index.owner_at(filename, byte) else { return vec![] };
        let Some(content_id) = index.content_id_of_file(filename) else { return vec![] };
        let commit_id = index.commit_id_of_entity(src_id);

        if let Some(class_id) = index.resolve_class(receiver_name, content_id) {
            if let Some(tgt_id) = index.resolve_method(class_id, method_name) {
                if src_id != tgt_id {
                    return vec![dep_at_position(src_id, tgt_id, DepKind::Call, byte, row, col, commit_id)];
                }
            }
        } else if let Some(var_types) = index.method_to_var_types_map.get(&src_id) {
            if let Some(class_ids) = var_types.get(receiver_name) {
                return class_ids.iter()
                    .filter_map(|&class_id| {
                        let tgt_id = index.resolve_method(class_id, method_name)?;
                        (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Infer, byte, row, col, commit_id))
                    })
                    .collect();
            }
        }
        vec![]
    }

    fn emit_extend_dep(
        &self, filename: &str, byte: usize, row: usize, col: usize, base_name: &str, index: &EntityIndex,
    ) -> Option<EntityDep> {
        let class_id = index.owner_at(filename, byte)?;
        let content_id = index.content_id_of_file(filename)?;
        let base_id = index.resolve_class(base_name, content_id)?;
        (class_id != base_id).then(|| dep_at_position(class_id, base_id, DepKind::Extend, byte, row, col, index.commit_id_of_entity(class_id)))
    }

    fn emit_import_dep(
        &self,
        filename: &str,
        byte: usize, row: usize, col: usize,
        module_text: &str,
        all_sources: &HashMap<String, String>,
        index: &EntityIndex,
    ) -> Option<EntityDep> {
        let src_id = index.file_entity_for(filename)?;
        let resolved_path = resolve_python_module_path(module_text, all_sources)?;
        if resolved_path == "__init__.py" || resolved_path.ends_with("/__init__.py") {
            return None;
        }
        let tgt_id = index.file_entity_for(&resolved_path)?;
        (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Import, byte, row, col, index.commit_id_of_entity(src_id)))
    }

    fn emit_create_dep(
        &self, filename: &str, byte: usize, row: usize, col: usize, class_name: &str, index: &EntityIndex,
    ) -> Option<EntityDep> {
        if matches!(class_name, "self" | "cls" | "super") { return None; }
        let content_id = index.content_id_of_file(filename)?;
        let tgt_id = index.resolve_class(class_name, content_id)?;
        let src_id = index.owner_at(filename, byte)?;
        (src_id != tgt_id).then(|| dep_at_position(src_id, tgt_id, DepKind::Create, byte, row, col, index.commit_id_of_entity(src_id)))
    }

    fn infer_python_returns(&self, sources: &HashMap<String, String>, index: &EntityIndex) -> ReturnSolverResult {
        let mut inference = self.collect_python_return_inference(sources, index);
        let initial_result = ReturnSolver::solve_with_member_resolver(&inference.facts, ReturnSolverOptions::default(), Some(index));
        self.add_call_site_field_facts(sources, index, &initial_result, &mut inference);
        ReturnSolver::solve_with_member_resolver(&inference.facts, ReturnSolverOptions::default(), Some(index))
    }

    fn collect_python_return_inference(&self, sources: &HashMap<String, String>, index: &EntityIndex) -> PythonReturnInference {
        let mut inference = PythonReturnInference { facts: ReturnFacts::default(), param_names: HashMap::new(), field_assignments: HashMap::new() };
        for (filename, content) in sources {
            self.extend_return_inference(filename, content, index, &mut inference);
        }
        inference
    }

    fn extend_return_inference(&self, filename: &str, content: &str, index: &EntityIndex, inference: &mut PythonReturnInference) {
        let facts = self.extract_return_facts_from_ast(filename, content, index);
        inference.facts.returns.extend(facts.returns);
        inference.facts.assignments.extend(facts.assignments);
        inference.facts.fields.extend(facts.fields);
        inference.param_names.extend(self.extract_param_names_from_ast(filename, content, index));
        for assignment in self.extract_field_param_assignments_from_ast(filename, content, index) {
            inference.field_assignments.entry(assignment.callable_id).or_default().push(assignment);
        }
    }

    fn add_call_site_field_facts(
        &self,
        sources: &HashMap<String, String>,
        index: &EntityIndex,
        initial_result: &ReturnSolverResult,
        inference: &mut PythonReturnInference,
    ) {
        for (filename, content) in sources {
            inference.facts.fields.extend(self.derive_field_facts_from_call_sites(
                filename,
                content,
                index,
                initial_result,
                &inference.param_names,
                &inference.field_assignments,
            ));
        }
    }

    fn build_enhancement_index(&self, sources: &HashMap<String, String>, entities: &[Entity], deps: &[EntityDep]) -> EntityIndex {
        let mut index = EntityIndex::build(sources, entities, deps);
        for (filename, content) in sources {
            self.index_base_classes_from_ast(filename, content, &mut index);
        }
        for (filename, content) in sources {
            self.index_var_types_from_ast(filename, content, &mut index);
        }
        index
    }

    fn emit_python_query_deps(
        &self,
        sources: &HashMap<String, String>,
        index: &EntityIndex,
        return_result: &ReturnSolverResult,
    ) -> (Vec<EntityDep>, HashSet<EntityId>) {
        let mut deps = Vec::new();
        let mut abstract_method_ids = HashSet::new();
        for (filename, content) in sources {
            match self.emit_typed_deps_for_file(filename, content, sources, index, &mut abstract_method_ids) {
                Ok(new_deps) => deps.extend(new_deps),
                Err(e) => log::warn!("python dep enhancement skipped {}: {}", filename, e),
            }
            deps.extend(self.emit_return_inferred_receiver_calls_for_file(filename, content, index, return_result));
        }
        (deps, abstract_method_ids)
    }
}

impl DepEnhancer for PythonQueryEnhancer {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let index = self.build_enhancement_index(sources, entities, &deps);
        let return_result = self.infer_python_returns(sources, &index);
        let mut deps = filter_scope_bleed_false_positives(deps, &index);
        deps.retain(|d| d.kind != DepKind::Create);

        let (new_deps, abstract_method_ids) = self.emit_python_query_deps(sources, &index, &return_result);
        deps.extend(new_deps);
        deps.extend(derive_override_deps(&index, &abstract_method_ids));
        deps.retain(|d| d.kind != DepKind::Import || !index.is_package_init_file(d.tgt));
        dedup_edges(&mut deps);
        let call_pairs: HashSet<(EntityId, EntityId)> =
            deps.iter().filter(|d| d.kind == DepKind::Call).map(|d| (d.src, d.tgt)).collect();
        deps.retain(|d| d.kind != DepKind::Infer || !call_pairs.contains(&(d.src, d.tgt)));
        deps
    }
}

fn filter_scope_bleed_false_positives(deps: Vec<EntityDep>, index: &EntityIndex) -> Vec<EntityDep> {
    deps.into_iter().filter(|d| !is_scope_bleed_false_positive(d, index)).collect()
}

fn is_scope_bleed_false_positive(dep: &EntityDep, index: &EntityIndex) -> bool {
    let Some(src) = index.entity(dep.src) else { return false };
    let Some(tgt) = index.entity(dep.tgt) else { return false };

    if src.kind == EntityKind::Field && tgt.kind == EntityKind::Class {
        return true;
    }

    let dep_row = dep.position.row();
    let at_src_definition = dep_row == src.code.start.row || dep_row == src.code.end.row;
    if !at_src_definition {
        return false;
    }

    let sibling_methods = src.kind == EntityKind::Method
        && tgt.kind == EntityKind::Method
        && src.parent_id.is_some()
        && src.parent_id == tgt.parent_id;

    let method_referencing_own_class = src.kind == EntityKind::Method
        && tgt.kind == EntityKind::Class
        && src.parent_id == Some(tgt.id);

    let field_referencing_sibling_or_parent = src.kind == EntityKind::Field
        && tgt.kind == EntityKind::Method
        && src.parent_id.is_some()
        && (src.parent_id == tgt.parent_id || src.parent_id == Some(tgt.id));

    sibling_methods || method_referencing_own_class || field_referencing_sibling_or_parent
}

fn derive_override_deps(index: &EntityIndex, abstract_method_ids: &HashSet<EntityId>) -> Vec<EntityDep> {
    let mut result = Vec::new();

    for &abstract_id in abstract_method_ids {
        let Some(abstract_entity) = index.entity(abstract_id) else { continue };
        let Some(abstract_class_id) = abstract_entity.parent_id else { continue };

        let mut visited: HashSet<EntityId> = HashSet::new();
        let mut queue: VecDeque<EntityId> = VecDeque::new();
        queue.push_back(abstract_class_id);

        while let Some(class_id) = queue.pop_front() {
            if !visited.insert(class_id) { continue; }

            if class_id != abstract_class_id {
                if let Some(&concrete_id) = index.class_to_methods_map
                    .get(&class_id)
                    .and_then(|m| m.get(&abstract_entity.name))
                {
                    if concrete_id != abstract_id {
                        result.push(dep_at_row(
                            concrete_id,
                            abstract_id,
                            DepKind::Override,
                            0,
                            index.commit_id_of_entity(concrete_id),
                        ));
                    }
                }
            }

            if let Some(subclasses) = index.class_to_subclasses_map.get(&class_id) {
                queue.extend(subclasses.iter().copied());
            }
        }
    }

    result
}

fn identifier_arguments(arguments: tree_sitter::Node, bytes: &[u8]) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut named_index = 0;
    let mut cursor = arguments.walk();
    for child in arguments.children(&mut cursor) {
        if !child.is_named() {
            continue;
        }
        if child.kind() == "identifier" {
            if let Ok(name) = child.utf8_text(bytes) {
                result.push((named_index, name.to_string()));
            }
        }
        named_index += 1;
    }
    result
}

fn callable_owner_at(filename: &str, byte: usize, index: &EntityIndex) -> Option<EntityId> {
    let owner_id = index.owner_at(filename, byte)?;
    if index.entity(owner_id).map_or(false, |e| e.kind == EntityKind::Field) {
        index.enclosing_callable_of(owner_id)
    } else {
        Some(owner_id)
    }
}

fn field_fact_call_context<'tree>(
    filename: &str,
    node: tree_sitter::Node<'tree>,
    bytes: &'tree [u8],
    index: &EntityIndex,
) -> Option<FieldFactCallContext<'tree>> {
    if node.kind() != "call" { return None }
    let arguments = node.child_by_field_name("arguments")?;
    Some(FieldFactCallContext {
        scope_id: index.owner_at(filename, node.start_byte())?,
        function: node.child_by_field_name("function")?,
        arguments: identifier_arguments(arguments, bytes),
    })
}

fn field_facts_for_target(
    target_callable_id: EntityId,
    scope_id: EntityId,
    arguments: &[(usize, String)],
    return_result: &ReturnSolverResult,
    param_names: &HashMap<EntityId, Vec<String>>,
    field_assignments: &HashMap<EntityId, Vec<FieldParamAssignment>>,
) -> Vec<FieldFact> {
    let Some(params) = param_names.get(&target_callable_id) else { return vec![] };
    let Some(assignments) = field_assignments.get(&target_callable_id) else { return vec![] };
    arguments
        .iter()
        .flat_map(|(arg_index, arg_name)| field_facts_for_argument(scope_id, *arg_index, arg_name, params, assignments, return_result))
        .collect()
}

fn field_facts_for_argument(
    scope_id: EntityId,
    arg_index: usize,
    arg_name: &str,
    params: &[String],
    assignments: &[FieldParamAssignment],
    return_result: &ReturnSolverResult,
) -> Vec<FieldFact> {
    let Some(param_name) = params.get(param_index_for_arg(arg_index, params)) else { return vec![] };
    let Some(arg_summary) = return_result.variable_summaries.get(&(scope_id, arg_name.to_string())) else { return vec![] };
    field_facts_from_summary(param_name, arg_summary, assignments)
}

fn param_index_for_arg(arg_index: usize, params: &[String]) -> usize {
    if params.first().map_or(false, |p| matches!(p.as_str(), "self" | "cls")) {
        arg_index + 1
    } else {
        arg_index
    }
}

fn field_facts_from_summary(
    param_name: &str,
    arg_summary: &crate::enhancement::returns::ReturnSummary,
    assignments: &[FieldParamAssignment],
) -> Vec<FieldFact> {
    assignments
        .iter()
        .filter(|assignment| assignment.param_name == param_name)
        .flat_map(|assignment| field_facts_for_assignment(assignment, arg_summary))
        .collect()
}

fn field_facts_for_assignment(
    assignment: &FieldParamAssignment,
    arg_summary: &crate::enhancement::returns::ReturnSummary,
) -> Vec<FieldFact> {
    arg_summary
        .returns()
        .map(|arg_type| FieldFact {
            class_id: assignment.class_id,
            field_name: assignment.field_name.clone(),
            type_id: arg_type.type_id,
            evidence: TypeEvidence::high(TypeEvidenceSource::FieldType),
        })
        .collect()
}

fn constructor_call_target(
    function: tree_sitter::Node,
    bytes: &[u8],
    content_id: crate::core::ContentId,
    index: &EntityIndex,
) -> Vec<(EntityId, EntityId)> {
    let Ok(name) = function.utf8_text(bytes) else { return vec![] };
    let Some(class_id) = index.resolve_class(name, content_id) else { return vec![] };
    index.resolve_method(class_id, "__init__")
        .map(|init_id| vec![(init_id, class_id)])
        .unwrap_or_default()
}

fn method_call_targets(
    function: tree_sitter::Node,
    bytes: &[u8],
    content_id: crate::core::ContentId,
    scope_id: EntityId,
    index: &EntityIndex,
    return_result: &ReturnSolverResult,
) -> Vec<(EntityId, EntityId)> {
    let Some((receiver_name, method_name)) = attribute_call_parts(function, bytes) else { return vec![] };
    if let Some(class_id) = index.resolve_class(receiver_name, content_id) {
        return method_target_for_class(class_id, method_name, index);
    }
    method_targets_for_variable(scope_id, receiver_name, method_name, index, return_result)
}

fn attribute_call_parts<'tree>(function: tree_sitter::Node<'tree>, bytes: &'tree [u8]) -> Option<(&'tree str, &'tree str)> {
    let object = function.child_by_field_name("object")?;
    let attribute = function.child_by_field_name("attribute")?;
    if object.kind() != "identifier" {
        return None;
    }
    Some((object.utf8_text(bytes).ok()?, attribute.utf8_text(bytes).ok()?))
}

fn method_target_for_class(class_id: EntityId, method_name: &str, index: &EntityIndex) -> Vec<(EntityId, EntityId)> {
    index.resolve_method(class_id, method_name)
        .map(|method_id| vec![(method_id, class_id)])
        .unwrap_or_default()
}

fn method_targets_for_variable(
    scope_id: EntityId,
    receiver_name: &str,
    method_name: &str,
    index: &EntityIndex,
    return_result: &ReturnSolverResult,
) -> Vec<(EntityId, EntityId)> {
    return_result
        .variable_summaries
        .get(&(scope_id, receiver_name.to_string()))
        .into_iter()
        .flat_map(|summary| summary.returns())
        .filter_map(|receiver_type| {
            let method_id = index.resolve_method(receiver_type.type_id, method_name)?;
            Some((method_id, receiver_type.type_id))
        })
        .collect()
}

fn walk_tree<'a, F: FnMut(tree_sitter::Node<'a>)>(node: tree_sitter::Node<'a>, f: &mut F) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree(child, f);
    }
}

fn set_receiver_capture<'tree>(
    capture_name: &str,
    node: tree_sitter::Node<'tree>,
    bytes: &'tree [u8],
    captures: &mut ReceiverCaptures<'tree>,
) {
    let point = node.start_position();
    match capture_name {
        "call.class_recv" => {
            captures.receiver_byte = Some(node.start_byte());
            captures.receiver_row = Some(point.row);
            captures.receiver_col = Some(point.column);
            captures.receiver_name = node.utf8_text(bytes).ok();
        }
        "call.class_method" => captures.method_name = node.utf8_text(bytes).ok(),
        "call.self_field_recv" => {
            captures.field_byte = Some(node.start_byte());
            captures.field_row = Some(point.row);
            captures.field_col = Some(point.column);
            captures.field_name = node.utf8_text(bytes).ok();
        }
        "call.self_field_method" => captures.field_method_name = node.utf8_text(bytes).ok(),
        _ => {}
    }
}

fn receiver_call_from_variable(filename: &str, captures: &ReceiverCaptures, index: &EntityIndex) -> Option<ReceiverCallFact> {
    let receiver_name = captures.receiver_name?;
    if matches!(receiver_name, "self" | "cls" | "super") {
        return None;
    }
    let byte = captures.receiver_byte?;
    Some(ReceiverCallFact {
        caller_id: index.owner_at(filename, byte)?,
        receiver: ReturnExpr::Variable(receiver_name.to_string()),
        method_name: captures.method_name?.to_string(),
        position: Position::new(byte, captures.receiver_row?, captures.receiver_col?),
    })
}

fn receiver_call_from_self_field(filename: &str, captures: &ReceiverCaptures, index: &EntityIndex) -> Option<ReceiverCallFact> {
    let byte = captures.field_byte?;
    let caller_id = index.owner_at(filename, byte)?;
    let class_id = index.enclosing_class_of(caller_id)?;
    Some(ReceiverCallFact {
        caller_id,
        receiver: ReturnExpr::Field { class_id, field_name: captures.field_name?.to_string() },
        method_name: captures.field_method_name?.to_string(),
        position: Position::new(byte, captures.field_row?, captures.field_col?),
    })
}

fn self_field_receiver_calls(
    filename: &str,
    root_node: tree_sitter::Node,
    bytes: &[u8],
    index: &EntityIndex,
    receiver_calls: &mut Vec<ReceiverCallFact>,
) {
    walk_tree(root_node, &mut |node| {
        if let Some(call) = nested_self_field_call(filename, node, bytes, index) {
            receiver_calls.push(call);
        }
    });
}

fn nested_self_field_call(filename: &str, node: tree_sitter::Node, bytes: &[u8], index: &EntityIndex) -> Option<ReceiverCallFact> {
    if node.kind() != "call" { return None }
    let function = node.child_by_field_name("function")?;
    let outer_object = function.child_by_field_name("object")?;
    let outer_attribute = function.child_by_field_name("attribute")?;
    let inner_object = outer_object.child_by_field_name("object")?;
    let inner_attribute = outer_object.child_by_field_name("attribute")?;
    if function.kind() != "attribute" || outer_object.kind() != "attribute" || inner_object.utf8_text(bytes).ok()? != "self" {
        return None;
    }
    let caller_id = index.owner_at(filename, inner_attribute.start_byte())?;
    let class_id = index.enclosing_class_of(caller_id)?;
    let point = inner_attribute.start_position();
    Some(ReceiverCallFact {
        caller_id,
        receiver: ReturnExpr::Field { class_id, field_name: inner_attribute.utf8_text(bytes).ok()?.to_string() },
        method_name: outer_attribute.utf8_text(bytes).ok()?.to_string(),
        position: Position::new(inner_attribute.start_byte(), point.row, point.column),
    })
}

fn inferred_receiver_dep(candidate: ReceiverCallCandidate, index: &EntityIndex) -> Option<EntityDep> {
    if candidate.caller_id == candidate.callee_id {
        return None;
    }
    Some(dep_at_position(
        candidate.caller_id,
        candidate.callee_id,
        DepKind::Infer,
        candidate.position.byte,
        candidate.position.row,
        candidate.position.column,
        index.commit_id_of_entity(candidate.caller_id),
    ))
}

fn resolve_python_module_path(module_text: &str, sources: &HashMap<String, String>) -> Option<String> {
    let path = module_text.replace('.', "/");

    let as_file = format!("{}.py", path);
    if sources.contains_key(&as_file) { return Some(as_file); }

    let as_package = format!("{}/__init__.py", path);
    if sources.contains_key(&as_package) { return Some(as_package); }

    let last_component = module_text.split('.').last()?;
    let suffix = format!("{}.py", last_component);
    sources.keys().find(|k| k.ends_with(&suffix)).cloned()
}

#[derive(Debug)]
pub struct PythonDataclassHeuristic;

impl DepEnhancer for PythonDataclassHeuristic {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[Entity],
        mut deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let index = EntityIndex::build(sources, entities, &deps);

        for (filename, content) in sources {
            if !filename.ends_with(".py") { continue; }
            let lines: Vec<&str> = content.lines().collect();
            let file_cid = crate::core::ContentId::from_content(content);

            for entity in entities {
                if entity.kind != EntityKind::Class || entity.content_id != file_cid { continue; }
                if !has_dataclass_decorator(entity, &lines) { continue; }

                for &child_id in index.children_of(entity.id) {
                    let Some(child) = index.entity(child_id) else { continue };
                    if child.kind != EntityKind::Field { continue; }

                    let row = child.code.start.row;
                    let Some(line) = lines.get(row) else { continue };

                    for class_name in class_names_from_type_annotation(line) {
                        for &class_id in index.classes_named(&class_name) {
                            if class_id == entity.id { continue; }
                            deps.push(dep_at_row(
                                child_id,
                                class_id,
                                DepKind::Use,
                                row,
                                index.commit_id_of_entity(child_id),
                            ));
                        }
                    }
                }
            }
        }

        dedup_edges(&mut deps);
        deps
    }
}

fn has_dataclass_decorator(class_entity: &Entity, lines: &[&str]) -> bool {
    let check_from = class_entity.code.start.row.saturating_sub(5);
    for i in (check_from..class_entity.code.start.row).rev() {
        let line = lines[i].trim();
        if line == "@dataclass" || line.starts_with("@dataclass(") || line == "@dataclasses.dataclass" {
            return true;
        }
        if !line.starts_with('@') && !line.is_empty() {
            break;
        }
    }
    false
}

fn class_names_from_type_annotation(line: &str) -> Vec<String> {
    let Some(colon_pos) = line.find(':') else { return vec![] };
    let annotation = line[colon_pos + 1..].split('=').next().unwrap_or("");

    let mut names = Vec::new();
    let mut current_word = String::new();
    for ch in annotation.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current_word.push(ch);
        } else {
            if !current_word.is_empty() {
                let word = std::mem::take(&mut current_word);
                if word.chars().next().map_or(false, |c| c.is_uppercase()) {
                    names.push(word);
                }
            }
        }
    }
    if !current_word.is_empty() && current_word.chars().next().map_or(false, |c| c.is_uppercase()) {
        names.push(current_word);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ContentId, Dep, EntityKind, PartialPosition, Position, PseudoCommitId, SimpleEntityId, Span};

    fn make_entity(
        name: &str,
        kind: EntityKind,
        parent_id: Option<EntityId>,
        content_id: ContentId,
        start_byte: usize,
        end_byte: usize,
    ) -> Entity {
        let code = Span::new(Position::new(start_byte, 0, 0), Position::new(end_byte, 0, 0));
        let simple_id = SimpleEntityId::new(None, name, kind);
        Entity::new(parent_id, name.to_string(), kind, code, None, content_id, simple_id)
    }

    #[test]
    fn sibling_method_at_definition_row_is_scope_bleed() {
        let content = "class A:\n    def foo(self): pass\n    def bar(self): pass\n";
        let cid = ContentId::from_content(content);
        let sources = HashMap::from([("a.py".to_string(), content.to_string())]);

        let file_e = make_entity("a.py", EntityKind::File, None, cid, 0, content.len());
        let class_e = make_entity("A", EntityKind::Class, Some(file_e.id), cid, 0, content.len());
        let foo_e = make_entity("foo", EntityKind::Method, Some(class_e.id), cid, 9, 30);
        let bar_e = make_entity("bar", EntityKind::Method, Some(class_e.id), cid, 31, 51);

        let entities = vec![file_e, class_e, foo_e.clone(), bar_e.clone()];
        let index = EntityIndex::build(&sources, &entities, &[]);

        let dep = Dep::new(foo_e.id, bar_e.id, DepKind::Call, PartialPosition::Row(foo_e.code.start.row), PseudoCommitId::WorkDir);
        let filtered = filter_scope_bleed_false_positives(vec![dep], &index);
        assert!(filtered.is_empty());
    }

    #[test]
    fn method_referencing_own_class_at_definition_row_is_scope_bleed() {
        let content = "class A:\n    def foo(self): pass\n";
        let cid = ContentId::from_content(content);
        let sources = HashMap::from([("a.py".to_string(), content.to_string())]);

        let file_e = make_entity("a.py", EntityKind::File, None, cid, 0, content.len());
        let class_e = make_entity("A", EntityKind::Class, Some(file_e.id), cid, 0, content.len());
        let foo_e = make_entity("foo", EntityKind::Method, Some(class_e.id), cid, 9, content.len());

        let entities = vec![file_e, class_e.clone(), foo_e.clone()];
        let index = EntityIndex::build(&sources, &entities, &[]);

        let dep = Dep::new(foo_e.id, class_e.id, DepKind::Use, PartialPosition::Row(foo_e.code.start.row), PseudoCommitId::WorkDir);
        let filtered = filter_scope_bleed_false_positives(vec![dep], &index);
        assert!(filtered.is_empty());
    }

    #[test]
    fn dep_at_interior_row_survives_scope_bleed_filter() {
        let content = "class A:\n    def foo(self):\n        pass\n    def bar(self): pass\n";
        let cid = ContentId::from_content(content);
        let sources = HashMap::from([("a.py".to_string(), content.to_string())]);

        let file_e = make_entity("a.py", EntityKind::File, None, cid, 0, content.len());
        let class_e = make_entity("A", EntityKind::Class, Some(file_e.id), cid, 0, content.len());
        let foo_e = make_entity("foo", EntityKind::Method, Some(class_e.id), cid, 9, 35);
        let bar_e = make_entity("bar", EntityKind::Method, Some(class_e.id), cid, 36, 56);

        let entities = vec![file_e, class_e, foo_e.clone(), bar_e.clone()];
        let index = EntityIndex::build(&sources, &entities, &[]);

        let dep = Dep::new(foo_e.id, bar_e.id, DepKind::Call, PartialPosition::Row(2), PseudoCommitId::WorkDir);
        let filtered = filter_scope_bleed_false_positives(vec![dep], &index);
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn class_names_from_annotation_finds_uppercase_identifiers() {
        let names = class_names_from_type_annotation("    inventory: List[Station]");
        assert!(names.contains(&"List".to_string()));
        assert!(names.contains(&"Station".to_string()));
    }

    #[test]
    fn class_names_from_annotation_skips_lowercase() {
        let names = class_names_from_type_annotation("    count: int = 0");
        assert!(names.is_empty());
    }

    #[test]
    fn has_dataclass_decorator_detects_decorator() {
        let lines = vec!["@dataclass", "class Foo:"];
        let content_id = ContentId::from_content("x");
        let e = make_entity("Foo", EntityKind::Class, None, content_id, 0, 10);
        let e = Entity::new(e.parent_id, e.name, e.kind,
            Span::new(Position::new(0, 1, 0), Position::new(10, 1, 0)),
            None, content_id, e.simple_id);
        assert!(has_dataclass_decorator(&e, &lines));
    }

    #[test]
    fn has_dataclass_decorator_false_without_decorator() {
        let lines = vec!["class Foo:"];
        let content_id = ContentId::from_content("x");
        let e = make_entity("Foo", EntityKind::Class, None, content_id, 0, 10);
        assert!(!has_dataclass_decorator(&e, &lines));
    }

    #[test]
    fn infers_receiver_call_from_function_return_assignment() {
        let content = "\
class ConcreteService:
    def run(self):
        pass

def get_service():
    return ConcreteService()

def caller():
    svc = get_service()
    svc.run()
";
        let cid = ContentId::from_content(content);
        let sources = HashMap::from([("a.py".to_string(), content.to_string())]);

        let file_e = make_entity("a.py", EntityKind::File, None, cid, 0, content.len());
        let service_start = content.find("class ConcreteService").unwrap();
        let get_service_start = content.find("def get_service").unwrap();
        let caller_start = content.find("def caller").unwrap();
        let run_start = content.find("def run").unwrap();

        let service_e = make_entity(
            "ConcreteService",
            EntityKind::Class,
            Some(file_e.id),
            cid,
            service_start,
            get_service_start.saturating_sub(1),
        );
        let run_e = make_entity(
            "run",
            EntityKind::Method,
            Some(service_e.id),
            cid,
            run_start,
            get_service_start.saturating_sub(1),
        );
        let get_service_e = make_entity(
            "get_service",
            EntityKind::Function,
            Some(file_e.id),
            cid,
            get_service_start,
            caller_start.saturating_sub(1),
        );
        let caller_e = make_entity(
            "caller",
            EntityKind::Function,
            Some(file_e.id),
            cid,
            caller_start,
            content.len(),
        );
        let entities = vec![
            file_e,
            service_e,
            run_e.clone(),
            get_service_e,
            caller_e.clone(),
        ];
        let enhancer = PythonQueryEnhancer::new(
            tree_sitter_python::language(),
            include_str!("../../languages/python/deps.scm"),
        ).unwrap();

        let deps = enhancer.enhance(&sources, &entities, vec![]);

        assert!(
            deps.iter().any(|d| d.src == caller_e.id && d.tgt == run_e.id && d.kind == DepKind::Infer),
            "svc = get_service(); svc.run() should infer caller -> ConcreteService.run"
        );
    }

    #[test]
    fn infers_self_field_receiver_from_constructor_argument() {
        let content = "\
class Station:
    def get_name(self):
        return \"NYC\"

class Route:
    def __init__(self, origin):
        self.origin = origin

    def display_info(self):
        self.origin.get_name()

def main():
    station = Station()
    route = Route(station)
    route.display_info()
";
        let cid = ContentId::from_content(content);
        let sources = HashMap::from([("a.py".to_string(), content.to_string())]);

        let file_e = make_entity("a.py", EntityKind::File, None, cid, 0, content.len());
        let station_start = content.find("class Station").unwrap();
        let route_start = content.find("class Route").unwrap();
        let main_start = content.find("def main").unwrap();
        let station_get_name_start = content.find("def get_name").unwrap();
        let route_init_start = content.find("def __init__").unwrap();
        let route_display_start = content.find("def display_info").unwrap();
        let origin_field_start = content.find("origin = origin").unwrap();

        let station_e = make_entity("Station", EntityKind::Class, Some(file_e.id), cid, station_start, route_start.saturating_sub(1));
        let station_get_name_e = make_entity("get_name", EntityKind::Method, Some(station_e.id), cid, station_get_name_start, route_start.saturating_sub(1));
        let route_e = make_entity("Route", EntityKind::Class, Some(file_e.id), cid, route_start, main_start.saturating_sub(1));
        let route_init_e = make_entity("__init__", EntityKind::Constructor, Some(route_e.id), cid, route_init_start, route_display_start.saturating_sub(1));
        let origin_field_e = make_entity("origin", EntityKind::Field, Some(route_init_e.id), cid, origin_field_start, origin_field_start + "origin".len());
        let route_display_e = make_entity("display_info", EntityKind::Method, Some(route_e.id), cid, route_display_start, main_start.saturating_sub(1));
        let main_e = make_entity("main", EntityKind::Function, Some(file_e.id), cid, main_start, content.len());
        let entities = vec![
            file_e,
            station_e,
            station_get_name_e.clone(),
            route_e,
            route_init_e,
            origin_field_e,
            route_display_e.clone(),
            main_e,
        ];
        let enhancer = PythonQueryEnhancer::new(
            tree_sitter_python::language(),
            include_str!("../../languages/python/deps.scm"),
        ).unwrap();

        let deps = enhancer.enhance(&sources, &entities, vec![]);

        assert!(
            deps.iter().any(|d| d.src == route_display_e.id && d.tgt == station_get_name_e.id && d.kind == DepKind::Infer),
            "self.origin.get_name() should infer Route.display_info -> Station.get_name"
        );
    }
}
