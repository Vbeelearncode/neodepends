use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::RwLock;

use counter::Counter;
use itertools::Itertools;
use rayon::prelude::*;

use crate::core::Change;
use crate::core::ChangeKind;
use crate::core::Content;
use crate::core::Diff;
use crate::core::Entity;
use crate::core::EntityDep;
use crate::core::FileKey;
use crate::core::SimpleEntityId;
use crate::filesystem::FileReader;
use crate::filesystem::FileSystem;
use crate::languages::Lang;
use crate::resolution::ResolverManager;
use crate::spec::Filespec;
use crate::tagging::EntitySet;

pub struct Extractor {
    fs: FileSystem,
    file_level: bool,
    no_enhance: bool,
    use_heuristics: bool,
    resolver: ResolverManager,
    entity_sets: RwLock<HashMap<FileKey, EntitySet>>,
}

impl Extractor {
    pub fn new(fs: FileSystem, file_level: bool, no_enhance: bool, use_heuristics: bool) -> Self {
        Self { fs, file_level, no_enhance, use_heuristics, resolver: ResolverManager::empty(), entity_sets: Default::default() }
    }

    pub fn set_resolver(&mut self, resolver: ResolverManager) {
        self.resolver = resolver;
    }

    pub fn extract_entities(&self, spec: &Filespec) -> impl ParallelIterator<Item = Entity> + '_ {
        let files = self.fs.list(spec);
        self.ensure_entity_sets(files.files().iter().sorted().cloned().collect());

        files.into_files().into_par_iter().flat_map(|f| {
            self.entity_sets.read().unwrap().get(&f).unwrap().clone().into_entities_vec()
        })
    }

    pub fn extract_changes(&self, spec: &Filespec) -> impl ParallelIterator<Item = Change> + '_ {
        let diffs: Vec<_> = spec
            .commits
            .par_iter()
            .filter_map(|c| c.try_as_commit_id())
            .flat_map(|c| self.fs.diff(c, &spec.pathspec).unwrap())
            .collect();
        let files = diffs.iter().flat_map(|d| d.iter_file_keys().cloned()).collect();
        self.ensure_entity_sets(files);
        diffs.into_par_iter().flat_map(move |d| calc_changes(&self.entity_sets.read().unwrap(), &d))
    }

    pub fn extract_deps(&self, spec: &Filespec) -> impl ParallelIterator<Item = EntityDep> + '_ {
        let files = self.fs.list(spec);
        self.ensure_entity_sets(files.files().iter().cloned().collect());

        let deps: Vec<EntityDep> = self.resolver
            .resolve(&self.fs, &files)
            .into_par_iter()
            .map(|d| d.to_entity_dep(&self.entity_sets.read().unwrap()).unwrap())
            .filter(|d| !d.is_loop())
            .collect();

        let deps = if self.no_enhance { deps } else { self.enhance_deps(files.files(), deps) };
        deps.into_par_iter()
    }

    fn enhance_deps(&self, file_keys: &HashSet<FileKey>, mut deps: Vec<EntityDep>) -> Vec<EntityDep> {
        let all_entities: Vec<Entity> = {
            let entity_sets = self.entity_sets.read().unwrap();
            file_keys
                .iter()
                .filter_map(|fk| entity_sets.get(fk))
                .flat_map(|es| es.iter_entities().cloned())
                .collect()
        };

        let mut sources_by_lang: HashMap<Lang, HashMap<String, String>> = HashMap::new();
        for fk in file_keys {
            let Some(lang) = Lang::of(&fk.filename) else { continue };
            let has_query = lang.query_enhancer().is_some();
            let has_heuristics = self.use_heuristics && lang.heuristic_enhancer().is_some();
            if !has_query && !has_heuristics { continue };
            match self.fs.read(fk.content_id) {
                Ok(content) => { sources_by_lang.entry(lang).or_default().insert(fk.filename.clone(), content); }
                Err(e) => log::warn!("enhance_deps: could not read {}: {}", fk.filename, e),
            }
        }

        for (lang, sources) in &sources_by_lang {
            if let Some(enhancer) = lang.query_enhancer() {
                deps = enhancer.enhance(sources, &all_entities, deps);
            }
            if self.use_heuristics {
                if let Some(enhancer) = lang.heuristic_enhancer() {
                    deps = enhancer.enhance(sources, &all_entities, deps);
                }
            }
        }
        deps
    }

    pub fn extract_contents(&self, spec: &Filespec) -> impl ParallelIterator<Item = Content> + '_ {
        let content_ids: HashSet<_> =
            self.fs.list(spec).files().iter().map(|f| f.content_id).collect();
        content_ids.into_par_iter().map(|id| Content::new(id, self.fs.read(id).unwrap()))
    }

    fn ensure_entity_sets(&self, files: HashSet<FileKey>) {
        files.into_par_iter().for_each(|f| {
            if !self.entity_sets.read().unwrap().contains_key(&f) {
                let content = self.fs.read(f.content_id).unwrap();
                let lang = Lang::of(&f.filename).unwrap();
                let entity_set = lang.tagger().tag(&f.filename, &content, self.file_level);
                self.entity_sets.write().unwrap().insert(f, entity_set);
            }
        })
    }
}

fn calc_changes(entity_sets: &HashMap<FileKey, EntitySet>, diff: &Diff) -> Vec<Change> {
    let old_entity_set = diff.old.as_ref().map(|k| entity_sets.get(&k).unwrap());
    let new_entity_set = diff.new.as_ref().map(|k| entity_sets.get(&k).unwrap());

    let old_ids = old_entity_set.map(|s| s.count_simple_ids(diff.iter_old_spans()));
    let new_ids = new_entity_set.map(|s| s.count_simple_ids(diff.iter_new_spans()));

    let mut ids = HashSet::new();
    ids.extend(old_ids.iter().flat_map(|x| x.keys()));
    ids.extend(new_ids.iter().flat_map(|x| x.keys()));

    let change_kinds: HashMap<SimpleEntityId, ChangeKind> = ids
        .iter()
        .map(|id| {
            let in_old = old_ids.as_ref().map_or(false, |old_ids| old_ids.contains_key(id));
            let in_new = new_ids.as_ref().map_or(false, |new_ids| new_ids.contains_key(id));
            let kind = match (in_old, in_new) {
                (false, false) => panic!(),
                (false, true) => ChangeKind::Added,
                (true, false) => ChangeKind::Deleted,
                (true, true) => ChangeKind::Modified,
            };
            (*id, kind)
        })
        .collect();

    let old_counts: Counter<SimpleEntityId> = old_ids.into_iter().flatten().collect();
    let new_counts: Counter<SimpleEntityId> = new_ids.into_iter().flatten().collect();

    ids.iter()
        .map(|id| {
            Change::new(*id, diff.commit_id, change_kinds[id], old_counts[id], new_counts[id])
        })
        .collect()
}
