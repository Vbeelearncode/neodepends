//! Used to interface with Stack Graphs
//! 
//! See https://github.com/github/stack-graphs

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::time::Duration;

use tempfile::TempDir;

use anyhow::anyhow;
use anyhow::Result;
use stack_graphs::arena::Handle;
use stack_graphs::graph::File;
use stack_graphs::graph::Node;
use stack_graphs::graph::StackGraph;
use stack_graphs::partial::PartialPath;
use stack_graphs::partial::PartialPaths;
use stack_graphs::storage::SQLiteWriter;
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
use crate::core::FileDep;
use crate::core::FileEndpoint;
use crate::core::FileKey;
use crate::core::PartialPosition;
use crate::core::PseudoCommitId;
use crate::core::Span;
use crate::languages::Lang;
use crate::resolution::Resolver;
use crate::resolution::ResolverFactory;

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::EnumString, strum::VariantNames)]
#[strum(serialize_all = "kebab-case")]
pub enum StackGraphsPythonMode {
    /// Old behavior: all StackGraphs edges are emitted as Use.
    UseOnly,
    /// Python-only behavior: classify StackGraphs references using AST context into Import/Extend/Call/Create, otherwise Use.
    Ast,
}

/// Stack graph data built for a single source file.
struct BuiltFileGraph {
    file: Handle<File>,
    graph: StackGraph,
    partials: PartialPaths,
    paths: Vec<PartialPath>,
}

/// A Stack Graphs resolver.
///
/// See [Resolver].
pub struct StackGraphsResolver {
    commit_id: PseudoCommitId,
    lang: Lang,
    py_mode: StackGraphsPythonMode,
    sgl: Arc<StackGraphLanguage>,
    ref_timeout: Duration,
    _tmp_dir: TempDir,
    writer: Mutex<Option<SQLiteWriter>>,
    file_keys: RwLock<HashMap<String, FileKey>>,
    contents: RwLock<HashMap<String, String>>,
}

impl StackGraphsResolver {
    fn new(
        commit_id: PseudoCommitId,
        lang: Lang,
        py_mode: StackGraphsPythonMode,
        sgl: Arc<StackGraphLanguage>,
        ref_timeout: Duration,
    ) -> Self {
        let tmp_dir = TempDir::new().expect("failed to create temp dir for stack-graphs SQLite");
        let db_path = tmp_dir.path().join("sg.db");
        let writer = SQLiteWriter::open(&db_path).expect("failed to open on-disk SQLite for stack-graphs");
        Self {
            commit_id,
            lang,
            py_mode,
            sgl,
            ref_timeout,
            _tmp_dir: tmp_dir,
            writer: Mutex::new(Some(writer)),
            file_keys: Default::default(),
            contents: Default::default(),
        }
    }
}

impl Resolver for StackGraphsResolver {
    fn add_file(&self, filename: &str, content: &str) {
        // Skip duplicates — resolve() would panic on them anyway
        if self.file_keys.read().unwrap().contains_key(filename) {
            return;
        }

        if let Some(mut built) = build_file_graph(&self.sgl, filename, content) {
            let mut guard = self.writer.lock().unwrap();
            if let Some(ref mut writer) = *guard {
                if let Err(err) = writer.store_result_for_file(
                    &built.graph,
                    built.file,
                    "",
                    &mut built.partials,
                    &built.paths,
                ) {
                    log::warn!("stack-graphs: failed to store graph for '{}': {err}", filename);
                }
            }
        }

        let file_key = FileKey::from_content(filename.to_string(), content);
        self.file_keys.write().unwrap().insert(filename.to_string(), file_key);
        self.contents.write().unwrap().insert(filename.to_string(), content.to_string());
    }

    fn resolve(&self) -> Vec<FileDep> {
        let file_keys = self.file_keys.read().unwrap();
        let contents = self.contents.read().unwrap();
        let mut parse_cache: HashMap<String, Tree> = HashMap::new();

        let writer = self.writer.lock().unwrap().take().expect("resolve called twice");
        let mut reader = writer.into_reader();

        // Load all file graphs upfront to enumerate every reference node.
        // Paths are NOT loaded here; the SQLiteReader loads them on-demand
        // as the stitcher explores the path frontier.
        let file_paths: Vec<String> = {
            let files: Vec<String> = reader
                .list_all()
                .unwrap()
                .try_iter()
                .unwrap()
                .map(|e| e.unwrap().path.to_string_lossy().to_string())
                .collect();
            files
        };

        for file_path in &file_paths {
            if let Err(err) = reader.load_graph_for_file(file_path) {
                log::warn!("stack-graphs: failed to load graph for '{}': {err}", file_path);
            }
        }

        let all_reference_nodes: Vec<Handle<Node>> = {
            let (graph, _, _) = reader.get();
            graph.iter_nodes().filter(|&n| graph[n].is_reference()).collect()
        };

        // Pre-load all paths so incoming_paths is fully populated before stitching.
        // This ensures get_incoming_path_degree returns correct values, preventing
        // incorrect cycle-detection behavior in the stitcher.
        if let Err(e) = reader.load_all_paths(&stack_graphs::NoCancellation) {
            log::warn!("stack-graphs: failed to pre-load paths: {e}");
        }

        let commit_id = self.commit_id;
        let lang = self.lang;
        let py_mode = self.py_mode;
        let mut deps: Vec<FileDep> = Vec::new();
        let diag_config = StitcherConfig::default();
        let ref_timeout = self.ref_timeout;

        // Stitch one reference at a time to bound intermediate path memory per query.
        // All base paths are already pre-loaded by load_all_paths, so lazy-loading
        // during stitching is a no-op and incoming_paths is fully populated.
        for (ref_idx, &ref_node) in all_reference_nodes.iter().enumerate() {
            let cancellation = stack_graphs::CancelAfterDuration::new(ref_timeout);
            let stitching_res = ForwardPartialPathStitcher::find_all_complete_partial_paths(
                &mut reader,
                std::iter::once(ref_node),
                diag_config,
                &cancellation,
                |graph, _partials, path| {
                    let get_filename = |n: Handle<Node>| graph[graph[n].file().unwrap()].name().to_string();
                    let get_position = |n: Handle<Node>| {
                        PartialPosition::Whole(Span::from_lsp(&graph.source_info(n).unwrap().span).start)
                    };

                    let start_node_pos = get_position(path.start_node);
                    let end_node_pos = get_position(path.end_node);

                    let src_filename = get_filename(path.start_node);
                    let tgt_filename = get_filename(path.end_node);
                    let src_content = contents.get(&src_filename).map(|s| s.as_str()).unwrap_or("");
                    let tgt_content = contents.get(&tgt_filename).map(|s| s.as_str()).unwrap_or("");
                    let src_byte = start_node_pos.byte().unwrap_or(0);
                    let tgt_byte = end_node_pos.byte().unwrap_or(0);

                    let kind = match py_mode {
                        StackGraphsPythonMode::UseOnly => Some(DepKind::Use),
                        StackGraphsPythonMode::Ast => classify_stackgraph_dep(
                            lang,
                            &src_filename,
                            src_content,
                            src_byte,
                            &tgt_filename,
                            tgt_content,
                            tgt_byte,
                            &mut parse_cache,
                        ),
                    };
                    let Some(kind) = kind else {
                        return;
                    };

                    // Skip edges involving files outside the scan scope (e.g. stdlib refs).
                    let (Some(src_file_key), Some(tgt_file_key)) = (file_keys.get(&src_filename), file_keys.get(&tgt_filename)) else {
                        return;
                    };

                    deps.push(Dep::new(
                        FileEndpoint::new(src_file_key.clone(), start_node_pos),
                        FileEndpoint::new(tgt_file_key.clone(), end_node_pos),
                        kind,
                        start_node_pos,
                        commit_id,
                    ));
                },
            );
            if stitching_res.is_err() {
                let (graph, _, _) = reader.get();
                let src_file = graph[ref_node].file()
                    .map(|f| graph[f].name().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let src_line = graph.source_info(ref_node).map(|si| si.span.start.line).unwrap_or(0);
                log::warn!("[SG] ref #{ref_idx:05} CANCELLED  file={src_file}:{src_line}");
            }
        }

        deps
    }
}

impl Debug for StackGraphsResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StackGraphsResolver")
            .field("commit_id", &self.commit_id)
            .field("py_mode", &self.py_mode)
            .field("tsg_path", &self.sgl.tsg_path())
            .field("file_keys", &self.file_keys)
            .finish()
    }
}

/// A Stack Graphs resolver factory.
///
/// See [ResolverFactory].
#[derive(Debug)]
pub struct StackGraphsResolverFactory {
    py_mode: StackGraphsPythonMode,
    ref_timeout: Duration,
}

impl StackGraphsResolverFactory {
    pub fn new(py_mode: StackGraphsPythonMode, ref_timeout_secs: Option<u64>) -> Self {
        let ref_timeout = ref_timeout_secs.map(Duration::from_secs).unwrap_or(Duration::MAX);
        Self { py_mode, ref_timeout }
    }
}

impl ResolverFactory for StackGraphsResolverFactory {
    fn try_create(&self, commit_id: PseudoCommitId, lang: Lang) -> Option<Box<dyn Resolver>> {
        lang.sgl().map(|sgl| {
            Box::new(StackGraphsResolver::new(commit_id, lang, self.py_mode, sgl, self.ref_timeout))
                as Box<dyn Resolver>
        })
    }
}

/// Attempt to build a stack graph from a single source file.
///
/// Returns `None` if the file could not be parsed or indexed.
fn build_file_graph(sgl: &StackGraphLanguage, filename: &str, content: &str) -> Option<BuiltFileGraph> {
    let mut graph = StackGraph::new();
    let mut partials = PartialPaths::new();
    let mut paths = Vec::new();

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

    Some(BuiltFileGraph { file, graph, partials, paths })
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

/// Check whether `node` sits inside an `isinstance(obj, Type)` call.
/// Returns true when the reference originated from a type-check context,
/// which is architecturally significant (Use dependency).
fn python_in_isinstance_arg(node: TsNode, src: &[u8]) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if n.kind() == "argument_list" {
            if let Some(call) = n.parent() {
                if call.kind() == "call" {
                    if let Some(func) = call.child_by_field_name("function") {
                        if func.kind() == "identifier" {
                            if let Ok(name) = func.utf8_text(src) {
                                if name == "isinstance" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        cur = n.parent();
    }
    false
}

fn python_call_dep_kind(tgt_node: TsNode) -> DepKind {
    match tgt_node.parent().as_ref().map(TsNode::kind).unwrap_or("") {
        "class_definition" => DepKind::Create,
        "function_definition" => DepKind::Call,
        _ => DepKind::Use,
    }
}

fn python_in_parameters_context(node: TsNode) -> bool {
    let mut cur = Some(node);
    while let Some(n) = cur {
        match n.kind() {
            "parameters" | "lambda_parameters" => return true,
            "function_definition" | "class_definition" | "module" => return false,
            _ => cur = n.parent(),
        }
    }
    false
}

fn is_class_name_definition(node: TsNode) -> bool {
    if node.kind() != "identifier" {
        return false;
    }
    if let Some(parent) = node.parent() {
        if parent.kind() == "class_definition" {
            return parent.child_by_field_name("name").map_or(false, |n| n == node);
        }
    }
    false
}

fn classify_stackgraph_dep(
    lang: Lang,
    src_filename: &str,
    src_content: &str,
    src_byte: usize,
    tgt_filename: &str,
    tgt_content: &str,
    tgt_byte: usize,
    parse_cache: &mut HashMap<String, Tree>,
) -> Option<DepKind> {
    if lang != Lang::Python {
        return Some(DepKind::Use);
    }

    let src_tree = match ts_parse_cached(parse_cache, lang, src_filename, src_content) {
        Ok(t) => t.clone(),
        Err(_) => return Some(DepKind::Use),
    };
    let tgt_tree = match ts_parse_cached(parse_cache, lang, tgt_filename, tgt_content) {
        Ok(t) => t.clone(),
        Err(_) => return Some(DepKind::Use),
    };

    let src_root = src_tree.root_node();
    let tgt_root = tgt_tree.root_node();
    let src_node = ts_node_at_byte(src_root, src_byte);
    let tgt_node = ts_node_at_byte(tgt_root, tgt_byte);

    if python_in_parameters_context(tgt_node) {
        return None;
    }

    if python_in_import_context(src_node) {
        // Import-to-Class is noise; the Import-to-File dep is captured by another path.
        return if is_class_name_definition(tgt_node) {
            None
        } else {
            Some(DepKind::Import)
        };
    }

    // Suppress before class_bases so Extend doesn't land on import aliases (→ File entity).
    if python_in_import_context(tgt_node) {
        return None;
    }

    if python_in_class_bases(src_node) {
        return Some(DepKind::Extend);
    }

    if python_in_isinstance_arg(src_node, src_content.as_bytes()) {
        return Some(DepKind::Use);
    }

    if let Some(call) = python_call_context(src_node) {
        if python_is_in_call_function(call, src_byte) {
            return Some(python_call_dep_kind(tgt_node));
        }
    }

    Some(DepKind::Use)
}
