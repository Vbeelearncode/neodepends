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
use crate::enhancement::DepEnhancer;
use crate::enhancement::entity_index::dep_at_position;
use crate::enhancement::entity_index::dep_at_row;
use crate::enhancement::entity_index::dedup_edges;
use crate::enhancement::entity_index::EntityIndex;

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
}

impl DepEnhancer for PythonQueryEnhancer {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let mut index = EntityIndex::build(sources, entities, &deps);

        for (filename, content) in sources {
            self.index_base_classes_from_ast(filename, content, &mut index);
        }
        for (filename, content) in sources {
            self.index_var_types_from_ast(filename, content, &mut index);
        }

        let mut deps = filter_scope_bleed_false_positives(deps, &index);
        deps.retain(|d| d.kind != DepKind::Create);

        let mut abstract_method_ids: HashSet<EntityId> = HashSet::new();
        for (filename, content) in sources {
            match self.emit_typed_deps_for_file(filename, content, sources, &index, &mut abstract_method_ids) {
                Ok(new_deps) => deps.extend(new_deps),
                Err(e) => log::warn!("python dep enhancement skipped {}: {}", filename, e),
            }
        }

        deps.extend(derive_override_deps(&index, &abstract_method_ids));
        deps.retain(|d| d.kind != DepKind::Import || !index.is_package_init_file(d.tgt));
        dedup_edges(&mut deps);
        deps
    }
}

fn filter_scope_bleed_false_positives(deps: Vec<EntityDep>, index: &EntityIndex) -> Vec<EntityDep> {
    deps.into_iter().filter(|d| !is_scope_bleed_false_positive(d, index)).collect()
}

fn is_scope_bleed_false_positive(dep: &EntityDep, index: &EntityIndex) -> bool {
    let Some(src) = index.entity(dep.src) else { return false };
    let Some(tgt) = index.entity(dep.tgt) else { return false };

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
}
