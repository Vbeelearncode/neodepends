use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use itertools::Itertools;
use lazy_static::lazy_static;
use tree_sitter::Language;
use tree_sitter_stack_graphs::StackGraphLanguage;

use crate::enhancement::ChainedEnhancer;
use crate::enhancement::DepEnhancer;
use crate::enhancement::JavaConstructorHeuristic;
use crate::enhancement::JavaOverrideHeuristic;
use crate::enhancement::PythonDataclassHeuristic;
use crate::enhancement::PythonQueryEnhancer;
use crate::spec::Pathspec;
use crate::tagging::Tagger;

/// Each programming language supported by Neodepends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(strum::Display, strum::EnumString, strum::VariantNames)]
#[strum(serialize_all = "lowercase")]
pub enum Lang {
    C,
    Cpp,
    Go,
    Java,
    JavaScript,
    Kotlin,
    Python,
    Ruby,
    TypeScript,
}

impl Lang {
    /// Get the language for a filename.
    pub fn of<S: AsRef<str>>(filename: S) -> Option<Self> {
        LANG_TABLE.get_lang(filename)
    }

    /// Get the [Pathspec] associated with this language.
    #[allow(dead_code)]
    pub fn pathspec(&self) -> &Pathspec {
        &self.config().pathspec
    }

    /// Create a [Pathspec] that matches many languages.
    pub fn pathspec_many<I: IntoIterator<Item = Lang>>(langs: I) -> Pathspec {
        Pathspec::new(LANG_TABLE.patterns(langs))
    }

    /// Get the [Tagger] associated with this language.
    pub fn tagger(&self) -> &Tagger {
        &self.config().tagger
    }

    /// Get the [StackGraphLanguage] associated with this language.
    pub fn sgl(&self) -> Option<Arc<StackGraphLanguage>> {
        self.config().sgl.clone()
    }

    /// Get the Tree-sitter language for this language.
    ///
    /// Useful when we need to parse the source AST outside of the tagger.
    pub fn ts_language(&self) -> Language {
        match self {
            Lang::C => tree_sitter_c::language(),
            Lang::Cpp => tree_sitter_cpp::language(),
            Lang::Go => tree_sitter_go::language(),
            Lang::Java => tree_sitter_java::language(),
            Lang::JavaScript => tree_sitter_javascript::language(),
            Lang::Kotlin => tree_sitter_kotlin::language(),
            Lang::Python => tree_sitter_python::language(),
            Lang::Ruby => tree_sitter_ruby::language(),
            Lang::TypeScript => tree_sitter_typescript::language_typescript(),
        }
    }

    /// Get the name of this language according to Depends.
    ///
    /// Intended to be passed to Depends as a command-line argument.
    pub fn depends_lang(&self) -> Option<&str> {
        self.config().depends_lang
    }

    pub fn query_enhancer(&self) -> Option<Arc<dyn DepEnhancer>> {
        self.config().query_enhancer.clone()
    }

    pub fn heuristic_enhancer(&self) -> Option<Arc<dyn DepEnhancer>> {
        self.config().heuristic_enhancer.clone()
    }

    fn config(&self) -> &LangConfig {
        match &self {
            Lang::C => &C,
            Lang::Cpp => &CPP,
            Lang::Go => &GO,
            Lang::Java => &JAVA,
            Lang::JavaScript => &JAVASCRIPT,
            Lang::Kotlin => &KOTLIN,
            Lang::Python => &PYTHON,
            Lang::Ruby => &RUBY,
            Lang::TypeScript => &TYPESCRIPT,
        }
    }
}

struct LangConfig {
    pathspec: Pathspec,
    tagger: Tagger,
    sgl: Option<Arc<StackGraphLanguage>>,
    depends_lang: Option<&'static str>,
    query_enhancer: Option<Arc<dyn DepEnhancer>>,
    heuristic_enhancer: Option<Arc<dyn DepEnhancer>>,
}

impl LangConfig {
    fn new(
        language: Language,
        pathspec: Pathspec,
        tag_query: Option<&str>,
        tsg: Option<&str>,
        dep_query: Option<&str>,
        depends_lang: Option<&'static str>,
        heuristics: Vec<Arc<dyn DepEnhancer>>,
    ) -> Self {
        let tagger = Tagger::new(Some(language), tag_query);
        let sgl = tsg.map(|x| Arc::new(StackGraphLanguage::from_str(language, &x).unwrap()));

        let query_enhancer: Option<Arc<dyn DepEnhancer>> =
            dep_query.and_then(|q| match PythonQueryEnhancer::new(language, q) {
                Ok(e) => Some(Arc::new(e) as Arc<dyn DepEnhancer>),
                Err(err) => {
                    eprintln!("WARNING: failed to compile dep query for language: {err}");
                    None
                }
            });

        let heuristic_enhancer: Option<Arc<dyn DepEnhancer>> = match heuristics.len() {
            0 => None,
            1 => heuristics.into_iter().next(),
            _ => Some(Arc::new(ChainedEnhancer(heuristics)) as Arc<dyn DepEnhancer>),
        };

        Self { pathspec, tagger, sgl, depends_lang, query_enhancer, heuristic_enhancer }
    }
}

#[derive(Debug, Default, Clone)]
struct LangLookupTable {
    special_files: HashMap<String, Lang>,
    extensions: HashMap<String, Lang>,
    patterns: HashMap<Lang, Vec<String>>,
}

impl LangLookupTable {
    fn new() -> Self {
        Self::default()
    }

    fn get_lang<S: AsRef<str>>(&self, filename: S) -> Option<Lang> {
        self.special_files
            .get(filename.as_ref())
            .or_else(|| {
                filename
                    .as_ref()
                    .to_lowercase()
                    .split(".")
                    .last()
                    .and_then(|e| self.extensions.get(e))
            })
            .copied()
    }

    fn pathspec(&self, lang: Lang) -> Pathspec {
        Pathspec::from_vec(self.patterns.get(&lang).unwrap().clone())
    }

    fn patterns<I>(&self, langs: I) -> Vec<&String>
    where
        I: IntoIterator<Item = Lang>,
    {
        let res = langs.into_iter().flat_map(|l| self.patterns.get(&l)).flatten().collect_vec();

        match res.is_empty() {
            true => self.patterns.values().flatten().unique().collect(),
            false => res,
        }
    }

    fn insert_special_file(&mut self, lang: Lang, special: &str) {
        self.special_files.insert(special.to_lowercase(), lang);
        self.patterns.entry(lang).or_default().push(special.to_string());
    }

    fn insert_extension(&mut self, lang: Lang, ext: &str) {
        self.extensions.insert(ext.to_lowercase(), lang);
        self.patterns.entry(lang).or_default().push(format!("*.{}", ext));
    }
}

lazy_static! {
    static ref LANG_TABLE: LangLookupTable = {
        let mut table = LangLookupTable::new();
        table.insert_extension(Lang::C, "c");
        table.insert_extension(Lang::Cpp, "c++");
        table.insert_extension(Lang::Cpp, "cc");
        table.insert_extension(Lang::Cpp, "cpp");
        table.insert_extension(Lang::Cpp, "cxx");
        table.insert_extension(Lang::Cpp, "h++");
        table.insert_extension(Lang::Cpp, "hh");
        table.insert_extension(Lang::Cpp, "hpp");
        table.insert_extension(Lang::Cpp, "hxx");
        table.insert_extension(Lang::Go, "go");
        table.insert_extension(Lang::Java, "java");
        table.insert_extension(Lang::JavaScript, "js");
        table.insert_extension(Lang::Kotlin, "kt");
        table.insert_extension(Lang::Python, "py");
        table.insert_extension(Lang::Ruby, "rb");
        table.insert_extension(Lang::TypeScript, "ts");
        table.insert_special_file(Lang::TypeScript, "tsconfig.json");
        table
    };
    static ref C: LangConfig = LangConfig::new(
        tree_sitter_c::language(),
        LANG_TABLE.pathspec(Lang::C),
        None,
        None,
        None,
        Some("cpp"),
        vec![],
    );
    static ref CPP: LangConfig = LangConfig::new(
        tree_sitter_cpp::language(),
        LANG_TABLE.pathspec(Lang::Cpp),
        None,
        None,
        None,
        Some("cpp"),
        vec![],
    );
    static ref GO: LangConfig = LangConfig::new(
        tree_sitter_go::language(),
        LANG_TABLE.pathspec(Lang::Go),
        None,
        None,
        None,
        Some("go"),
        vec![],
    );
    static ref JAVA: LangConfig = LangConfig::new(
        tree_sitter_java::language(),
        LANG_TABLE.pathspec(Lang::Java),
        Some(include_str!("../languages/java/tags.scm")),
        Some(include_str!("../languages/java/stack-graphs.tsg")),
        None,
        Some("java"),
        vec![
            Arc::new(JavaConstructorHeuristic) as Arc<dyn DepEnhancer>,
            Arc::new(JavaOverrideHeuristic) as Arc<dyn DepEnhancer>,
        ],
    );
    static ref JAVASCRIPT: LangConfig = LangConfig::new(
        tree_sitter_javascript::language(),
        LANG_TABLE.pathspec(Lang::JavaScript),
        None,
        Some(include_str!("../languages/javascript/stack-graphs.tsg")),
        None,
        None,
        vec![],
    );
    static ref KOTLIN: LangConfig = LangConfig::new(
        tree_sitter_kotlin::language(),
        LANG_TABLE.pathspec(Lang::Kotlin),
        None,
        None,
        None,
        Some("kotlin"),
        vec![],
    );
    static ref PYTHON: LangConfig = LangConfig::new(
        tree_sitter_python::language(),
        LANG_TABLE.pathspec(Lang::Python),
        Some(include_str!("../languages/python/tags.scm")),
        Some(include_str!("../languages/python/stack-graphs.tsg")),
        Some(include_str!("../languages/python/deps.scm")),
        Some("python"),
        vec![
            Arc::new(PythonDataclassHeuristic) as Arc<dyn DepEnhancer>,
        ],
    );
    static ref RUBY: LangConfig = LangConfig::new(
        tree_sitter_ruby::language(),
        LANG_TABLE.pathspec(Lang::Ruby),
        None,
        Some(include_str!("../languages/ruby/stack-graphs.tsg")),
        None,
        Some("ruby"),
        vec![],
    );
    static ref TYPESCRIPT: LangConfig = LangConfig::new(
        tree_sitter_typescript::language_typescript(),
        LANG_TABLE.pathspec(Lang::TypeScript),
        None,
        Some(include_str!("../languages/typescript/stack-graphs.tsg")),
        None,
        None,
        vec![],
    );
}
