//! Used to interface with Stack Graphs
//! 
//! See https://github.com/github/stack-graphs

use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::RwLock;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Result;
use stack_graphs::arena::Handle;
use stack_graphs::graph::Node;
use stack_graphs::graph::StackGraph;
use stack_graphs::partial::PartialPath;
use stack_graphs::partial::PartialPaths;
use stack_graphs::stitching::Database;
use stack_graphs::stitching::DatabaseCandidates;
use stack_graphs::stitching::ForwardPartialPathStitcher;
use stack_graphs::stitching::StitcherConfig;
use tree_sitter::Node as TsNode;
use tree_sitter::Parser;
use tree_sitter::Tree;
use tree_sitter_graph::Variables;
use tree_sitter_stack_graphs::NoCancellation;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::core::Dep;
use crate::core::DepKind;
use crate::core::Entity;
use crate::core::EntityDep;
use crate::core::EntityKind;
use crate::core::FileDep;
use crate::core::FileEndpoint;
use crate::core::FileKey;
use crate::core::PartialPosition;
use crate::core::PseudoCommitId;
use crate::core::Span;
use crate::languages::Lang;
use crate::resolution::Resolver;
use crate::resolution::ResolverFactory;
use crate::tagging::EntitySet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString, strum::VariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum StackGraphsPythonMode {
    /// Old behavior: all StackGraphs edges are emitted as Use.
    UseOnly,
    /// Python-only behavior: classify StackGraphs references using AST context into Import/Extend/Call/Create, otherwise Use.
    Ast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString, strum::VariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum StackGraphsTypeScriptMode {
    /// Old behavior: all StackGraphs edges are emitted as Use.
    UseOnly,
    /// TypeScript/TSX behavior: classify StackGraphs references using AST context into Import/Annotation/Extend/Implement/Create/Call, otherwise Use.
    Ast,
}

/// A Stack Graphs resolver.
///
/// See [Resolver].
pub struct StackGraphsResolver {
    commit_id: PseudoCommitId,
    lang: Lang,
    py_mode: StackGraphsPythonMode,
    ts_mode: StackGraphsTypeScriptMode,
    sgl: Arc<StackGraphLanguage>,
    cache: Arc<SgCache>,
    files: RwLock<HashSet<FileKey>>,
}

impl StackGraphsResolver {
    fn new(
        commit_id: PseudoCommitId,
        lang: Lang,
        py_mode: StackGraphsPythonMode,
        ts_mode: StackGraphsTypeScriptMode,
        sgl: Arc<StackGraphLanguage>,
        cache: Arc<SgCache>,
    ) -> Self {
        Self { commit_id, lang, py_mode, ts_mode, sgl, cache, files: Default::default() }
    }
}

impl Resolver for StackGraphsResolver {
    fn add_file(&self, filename: &str, content: &str) {
        let file = FileKey::from_content(filename.to_string(), content);

        if !self.cache.contains(&file) {
            self.cache.insert(file.clone(), build(&self.sgl, filename, content));
        }

        self.files.write().unwrap().insert(file);
    }

    fn resolve(&self) -> Vec<FileDep> {
        let files = self.files.read().unwrap();
        let data = files.iter().filter_map(|f| self.cache.get(f).unwrap());
        resolve(data, self.commit_id, self.lang, self.py_mode, self.ts_mode)
    }
}

impl Debug for StackGraphsResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StackGraphsResolver")
            .field("commit_id", &self.commit_id)
            .field("py_mode", &self.py_mode)
            .field("ts_mode", &self.ts_mode)
            .field("tsg_path", &self.sgl.tsg_path())
            .field("cache", &self.cache)
            .field("files", &self.files)
            .finish()
    }
}

/// A Stack Graphs resolver factory.
///
/// See [ResolverFactory].
#[derive(Debug)]
pub struct StackGraphsResolverFactory {
    cache: Arc<SgCache>,
    py_mode: StackGraphsPythonMode,
    ts_mode: StackGraphsTypeScriptMode,
}

impl StackGraphsResolverFactory {
    pub fn new(py_mode: StackGraphsPythonMode, ts_mode: StackGraphsTypeScriptMode) -> Self {
        Self { cache: Arc::new(SgCache::new()), py_mode, ts_mode }
    }
}

impl ResolverFactory for StackGraphsResolverFactory {
    fn try_create(&self, commit_id: PseudoCommitId, lang: Lang) -> Option<Box<dyn Resolver>> {
        lang.sgl().map(|sgl| {
            Box::new(StackGraphsResolver::new(
                commit_id,
                lang,
                self.py_mode,
                self.ts_mode,
                sgl,
                self.cache.clone(),
            )) as Box<dyn Resolver>
        })
    }
}

/// Used to avoid duplicate stack graph calculations.
#[derive(Debug)]
struct SgCache {
    map: RwLock<HashMap<FileKey, Option<StackGraphData>>>,
}

impl SgCache {
    fn new() -> Self {
        Self { map: Default::default() }
    }

    fn contains(&self, key: &FileKey) -> bool {
        self.map.read().unwrap().contains_key(key)
    }

    fn get(&self, key: &FileKey) -> Option<Option<StackGraphData>> {
        self.map.read().unwrap().get(key).cloned()
    }

    fn insert(&self, key: FileKey, value: Option<StackGraphData>) {
        self.map.write().unwrap().insert(key, value);
    }
}

/// A stack graph representation that is able to be cloned and moved around for
/// caching purposes.
///
/// Intended to contain the stack graph of a single file.
#[derive(Debug, Clone)]
struct StackGraphData {
    file_key: FileKey,
    content: String,
    graph: stack_graphs::serde::StackGraph,
    paths: Vec<stack_graphs::serde::PartialPath>,
}

impl StackGraphData {
    fn new(
        file_key: FileKey,
        content: String,
        graph: StackGraph,
        mut partials: PartialPaths,
        paths: Vec<PartialPath>,
    ) -> Self {
        let paths = paths
            .iter()
            .map(|p| stack_graphs::serde::PartialPath::from_partial_path(&graph, &mut partials, p))
            .collect::<Vec<_>>();
        let graph = stack_graphs::serde::StackGraph::from_graph(&graph);
        Self { file_key, content, graph, paths }
    }
}

/// A stack graph representation that can be used to resolve dependencies.
///
/// Intended to contain the stack graphs of many files.
struct StackGraphEval {
    file_keys: HashMap<String, FileKey>,
    contents: HashMap<String, String>,
    graph: StackGraph,
    partials: PartialPaths,
    paths: Vec<PartialPath>,
}

impl StackGraphEval {
    fn from_data<I>(data: I) -> anyhow::Result<Self>
    where
        I: IntoIterator<Item = StackGraphData>,
    {
        let mut file_keys = HashMap::new();
        let mut contents = HashMap::new();
        let mut graph = StackGraph::new();
        let mut partials = PartialPaths::new();
        let mut paths = Vec::new();

        for portable in data {
            if file_keys.contains_key(&portable.file_key.filename) {
                bail!("duplicate filenames");
            }

            file_keys.insert(portable.file_key.filename.clone(), portable.file_key.clone());
            contents.insert(portable.file_key.filename.clone(), portable.content.clone());
            portable.graph.load_into(&mut graph)?;

            for path in &portable.paths {
                paths.push(path.to_partial_path(&mut graph, &mut partials)?);
            }
        }

        Ok(StackGraphEval { file_keys, contents, graph, partials, paths })
    }
}

/// Attempt to build a stack graph from a source file.
///
/// Returns None if a stack graph could not be built.
fn build(sgl: &StackGraphLanguage, filename: &str, content: &str) -> Option<StackGraphData> {
    let mut graph = StackGraph::new();
    let mut partials = PartialPaths::new();
    let mut paths = Vec::new();

    let file_key = FileKey::from_content(filename.to_string(), content);

    let file = graph.get_or_create_file(filename);
    let vars = Variables::new();
    sgl.build_stack_graph_into(&mut graph, file, content, &vars, &NoCancellation).ok()?;

    ForwardPartialPathStitcher::find_minimal_partial_path_set_in_file(
        &graph,
        &mut partials,
        file,
        StitcherConfig::default(),
        &stack_graphs::NoCancellation,
        |_, _, p| {
            paths.push(p.clone());
        },
    )
    .ok()?;

    Some(StackGraphData::new(file_key, content.to_string(), graph, partials, paths))
}

fn ts_parse_cached<'a>(
    cache: &'a mut HashMap<String, Tree>,
    lang: Lang,
    filename: &str,
    content: &str,
) -> Result<&'a Tree> {
    if cache.contains_key(filename) {
        return Ok(cache.get(filename).unwrap());
    }
    let mut parser = Parser::new();
    parser.set_language(lang.ts_language())?;
    let tree = parser.parse(content, None).ok_or_else(|| anyhow!("failed to parse"))?;
    cache.insert(filename.to_string(), tree);
    Ok(cache.get(filename).unwrap())
}

fn ts_node_at_byte(root: TsNode, byte: usize) -> TsNode {
    root.descendant_for_byte_range(byte, byte.saturating_add(1)).unwrap_or(root)
}

fn python_in_import_context(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "import_statement" | "import_from_statement" => return true,
            _ => cur = n.parent(),
        }
    }
    false
}

fn python_in_class_bases(node: TsNode) -> bool {
    // Tree-sitter-python models base classes as an `argument_list` under `class_definition`:
    // class A(B, C):
    //         ^^^ argument_list
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "argument_list" {
            if let Some(p) = n.parent() {
                if p.kind() == "class_definition" {
                    return true;
                }
            }
        }
        cur = n.parent();
    }
    false
}

fn python_call_context(node: TsNode) -> Option<TsNode> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "call" {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

fn python_is_in_call_function(call: TsNode, byte: usize) -> bool {
    if let Some(fun) = call.child_by_field_name("function") {
        byte >= fun.start_byte() && byte < fun.end_byte()
    } else {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PyDefKind {
    Class,
    Function,
    Other,
}

fn python_def_kind_at(node: TsNode) -> PyDefKind {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "class_definition" => return PyDefKind::Class,
            "function_definition" => return PyDefKind::Function,
            _ => cur = n.parent(),
        }
    }
    PyDefKind::Other
}

// #region TypeScript / TSX AST-context helpers and classifier

fn ts_in_import_context(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "import_statement" | "import_clause" | "import_specifier" | "namespace_import"
            | "namespace_export" | "export_clause" | "export_specifier" => return true,
            "export_statement" => {
                if n.child_by_field_name("source").is_some() {
                    return true;
                }
                cur = n.parent();
            }
            _ => cur = n.parent(),
        }
    }
    false
}

fn ts_in_decorator(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "decorator" {
            return true;
        }
        cur = n.parent();
    }
    false
}

fn ts_in_extends_heritage(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "extends_clause" | "extends_type_clause" => return true,
            _ => cur = n.parent(),
        }
    }
    false
}

fn ts_in_implements_clause(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "implements_clause" {
            return true;
        }
        cur = n.parent();
    }
    false
}

fn ts_new_expression_context(node: TsNode) -> Option<TsNode> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "new_expression" {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

fn ts_is_in_new_constructor(new_expr: TsNode, byte: usize) -> bool {
    if let Some(ctor) = new_expr.child_by_field_name("constructor") {
        byte >= ctor.start_byte() && byte < ctor.end_byte()
    } else {
        false
    }
}

fn ts_call_context(node: TsNode) -> Option<TsNode> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

fn ts_is_in_call_function(call: TsNode, byte: usize) -> bool {
    if let Some(fun) = call.child_by_field_name("function") {
        byte >= fun.start_byte() && byte < fun.end_byte()
    } else {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TsDefKind {
    Class,
    Function,
    Other,
}

fn ts_def_kind_at(node: TsNode) -> TsDefKind {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "method_definition"
            | "method_signature"
            | "abstract_method_signature"
            | "function_declaration"
            | "generator_function_declaration"
            | "arrow_function" => return TsDefKind::Function,
            "class_declaration" | "abstract_class_declaration" => return TsDefKind::Class,
            _ => cur = n.parent(),
        }
    }
    TsDefKind::Other
}

fn classify_typescript_dep(
    lang: Lang,
    src_filename: &str,
    src_content: &str,
    src_byte: usize,
    tgt_filename: &str,
    tgt_content: &str,
    tgt_byte: usize,
    parse_cache: &mut HashMap<String, Tree>,
) -> DepKind {
    let src_tree = match ts_parse_cached(parse_cache, lang, src_filename, src_content) {
        Ok(t) => t.clone(),
        Err(_) => return DepKind::Use,
    };
    let tgt_tree = match ts_parse_cached(parse_cache, lang, tgt_filename, tgt_content) {
        Ok(t) => t.clone(),
        Err(_) => return DepKind::Use,
    };

    let src_root = src_tree.root_node();
    let tgt_root = tgt_tree.root_node();
    let src_node = ts_node_at_byte(src_root, src_byte);
    let tgt_node = ts_node_at_byte(tgt_root, tgt_byte);

    if ts_in_import_context(src_node) {
        return DepKind::Import;
    }

    if ts_in_decorator(src_node) {
        return DepKind::Annotation;
    }

    if ts_in_extends_heritage(src_node) {
        return DepKind::Extend;
    }

    if ts_in_implements_clause(src_node) {
        return DepKind::Implement;
    }

    if let Some(new_expr) = ts_new_expression_context(src_node) {
        if ts_is_in_new_constructor(new_expr, src_byte) {
            return DepKind::Create;
        }
    }

    if let Some(call) = ts_call_context(src_node) {
        if ts_is_in_call_function(call, src_byte) {
            return match ts_def_kind_at(tgt_node) {
                TsDefKind::Class => DepKind::Create,
                TsDefKind::Function | TsDefKind::Other => DepKind::Call,
            };
        }
    }

    DepKind::Use
}

// #endregion

fn classify_stackgraph_dep(
    lang: Lang,
    src_filename: &str,
    src_content: &str,
    src_byte: usize,
    tgt_filename: &str,
    tgt_content: &str,
    tgt_byte: usize,
    parse_cache: &mut HashMap<String, Tree>,
) -> DepKind {
    match lang {
        Lang::Python => {}
        Lang::TypeScript | Lang::Tsx => {
            return classify_typescript_dep(
                lang,
                src_filename,
                src_content,
                src_byte,
                tgt_filename,
                tgt_content,
                tgt_byte,
                parse_cache,
            );
        }
        _ => return DepKind::Use,
    }

    let src_tree = match ts_parse_cached(parse_cache, lang, src_filename, src_content) {
        Ok(t) => t.clone(),
        Err(_) => return DepKind::Use,
    };
    let tgt_tree = match ts_parse_cached(parse_cache, lang, tgt_filename, tgt_content) {
        Ok(t) => t.clone(),
        Err(_) => return DepKind::Use,
    };

    let src_root = src_tree.root_node();
    let tgt_root = tgt_tree.root_node();
    let src_node = ts_node_at_byte(src_root, src_byte);
    let tgt_node = ts_node_at_byte(tgt_root, tgt_byte);

    if python_in_import_context(src_node) {
        return DepKind::Import;
    }

    if python_in_class_bases(src_node) {
        return DepKind::Extend;
    }

    if let Some(call) = python_call_context(src_node) {
        if python_is_in_call_function(call, src_byte) {
            // If the call target resolves to a class definition, treat it as Create (constructor call).
            // Otherwise treat it as Call.
            return match python_def_kind_at(tgt_node) {
                PyDefKind::Class => DepKind::Create,
                _ => DepKind::Call,
            };
        }
    }

    DepKind::Use
}

/// Resolve file-level dependencies given for a collection of files.
fn resolve<I>(
    data: I,
    commit_id: PseudoCommitId,
    lang: Lang,
    py_mode: StackGraphsPythonMode,
    ts_mode: StackGraphsTypeScriptMode,
) -> Vec<FileDep>
where
    I: IntoIterator<Item = StackGraphData>,
{
    let mut references = Vec::new();
    let mut eval = StackGraphEval::from_data(data).unwrap();
    let mut parse_cache: HashMap<String, Tree> = HashMap::new();

    let mut db = Database::new();

    for path in &eval.paths {
        db.add_partial_path(&eval.graph, &mut eval.partials, path.clone());
    }

    let _stitching_res = ForwardPartialPathStitcher::find_all_complete_partial_paths(
        &mut DatabaseCandidates::new(&eval.graph, &mut eval.partials, &mut db),
        eval.graph.iter_nodes().filter(|&n| eval.graph[n].is_reference()),
        StitcherConfig::default(),
        &stack_graphs::NoCancellation,
        |_, _, p| {
            references.push(p.clone());
        },
    );

    let filename = |n: Handle<Node>| eval.graph[eval.graph[n].file().unwrap()].name().to_string();
    let file_key = |n: Handle<Node>| eval.file_keys.get(&filename(n)).unwrap().clone();
    let position = |n: Handle<Node>| {
        PartialPosition::Whole(Span::from_lsp(&eval.graph.source_info(n).unwrap().span).start)
    };

    references
        .into_iter()
        .map(|r| {
            let start_node_pos = position(r.start_node);
            let end_node_pos = position(r.end_node);

            let src_filename = filename(r.start_node);
            let tgt_filename = filename(r.end_node);
            let src_content = eval.contents.get(&src_filename).map(|s| s.as_str()).unwrap_or("");
            let tgt_content = eval.contents.get(&tgt_filename).map(|s| s.as_str()).unwrap_or("");
            let src_byte = start_node_pos.byte().unwrap_or(0);
            let tgt_byte = end_node_pos.byte().unwrap_or(0);

            let classify_enabled = match lang {
                Lang::Python => matches!(py_mode, StackGraphsPythonMode::Ast),
                Lang::TypeScript | Lang::Tsx => matches!(ts_mode, StackGraphsTypeScriptMode::Ast),
                _ => false,
            };
            let kind = if classify_enabled {
                classify_stackgraph_dep(
                    lang,
                    src_filename.as_str(),
                    src_content,
                    src_byte,
                    tgt_filename.as_str(),
                    tgt_content,
                    tgt_byte,
                    &mut parse_cache,
                )
            } else {
                DepKind::Use
            };

            Dep::new(
                FileEndpoint::new(file_key(r.start_node), start_node_pos),
                FileEndpoint::new(file_key(r.end_node), end_node_pos),
                kind,
                start_node_pos,
                commit_id,
            )
        })
        .collect()
}

// #region TypeScript / TSX false-positive filter

pub fn is_typescript_false_positive(
    lang: Lang,
    dep: &EntityDep,
    src_entity: &Entity,
    tgt_entity: &Entity,
    entity_sets: Option<&HashMap<FileKey, EntitySet>>,
) -> bool {
    if !matches!(lang, Lang::TypeScript | Lang::Tsx) {
        return false;
    }

    let src_is_member = matches!(
        src_entity.kind,
        EntityKind::Method | EntityKind::Field | EntityKind::Constructor
    );
    let tgt_is_member = matches!(
        tgt_entity.kind,
        EntityKind::Method | EntityKind::Field | EntityKind::Constructor
    );
    let tgt_is_container = matches!(tgt_entity.kind, EntityKind::Class | EntityKind::Interface);

    if src_is_member && tgt_is_container && src_entity.parent_id == Some(tgt_entity.id) {
        return true;
    }

    let dep_row = match dep.position {
        PartialPosition::Row(r) => r,
        PartialPosition::Whole(p) => p.row,
    };

    if src_is_member
        && tgt_is_member
        && src_entity.parent_id.is_some()
        && src_entity.parent_id == tgt_entity.parent_id
        && src_entity.id != tgt_entity.id
        && dep_row == src_entity.code.start.row
    {
        return true;
    }
    
    let src_is_declarable = matches!(
        src_entity.kind,
        EntityKind::Function | EntityKind::Method | EntityKind::Constructor | EntityKind::Field
    );
    if src_is_declarable
        && tgt_entity.kind == EntityKind::File
        && dep_row == src_entity.code.start.row
    {
        if let Some(sets) = entity_sets {
            if is_ancestor(src_entity, tgt_entity.id, sets) {
                return true;
            }
        }
    }

    false
}

fn is_ancestor(
    child: &Entity,
    ancestor_id: crate::core::EntityId,
    entity_sets: &HashMap<FileKey, EntitySet>,
) -> bool {
    let mut current = child.parent_id;
    while let Some(pid) = current {
        if pid == ancestor_id {
            return true;
        }
        let mut next = None;
        for set in entity_sets.values() {
            if let Some(e) = set.get_entity(&pid) {
                next = e.parent_id;
                break;
            }
        }
        current = next;
    }
    false
}

pub fn is_typescript_false_positive_with_sets(
    lang: Lang,
    dep: &EntityDep,
    entity_sets: &HashMap<FileKey, EntitySet>,
) -> bool {
    if !matches!(lang, Lang::TypeScript | Lang::Tsx) {
        return false;
    }
    let mut src_entity: Option<&Entity> = None;
    let mut tgt_entity: Option<&Entity> = None;
    for set in entity_sets.values() {
        if src_entity.is_none() {
            if let Some(e) = set.get_entity(&dep.src) {
                src_entity = Some(e);
            }
        }
        if tgt_entity.is_none() {
            if let Some(e) = set.get_entity(&dep.tgt) {
                tgt_entity = Some(e);
            }
        }
        if src_entity.is_some() && tgt_entity.is_some() {
            break;
        }
    }
    match (src_entity, tgt_entity) {
        (Some(src), Some(tgt)) => is_typescript_false_positive(lang, dep, src, tgt, Some(entity_sets)),
        _ => false,
    }
}

// #endregion
