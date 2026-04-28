use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::sync::Arc;

use crate::core::ContentId;
use crate::core::Dep;
use crate::core::DepKind;
use crate::core::Entity;
use crate::core::EntityDep;
use crate::core::EntityId;
use crate::core::EntityKind;
use crate::core::PartialPosition;
use crate::core::Position;
use crate::core::PseudoCommitId;
use crate::enhancement::DepEnhancer;

#[derive(Debug)]
pub struct ChainedEnhancer(pub Vec<Arc<dyn DepEnhancer>>);

impl DepEnhancer for ChainedEnhancer {
    fn enhance(
        &self,
        files: &HashMap<String, String>,
        entities: &[Entity],
        mut deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        for enhancer in &self.0 {
            deps = enhancer.enhance(files, entities, deps);
        }
        deps
    }
}

struct EntityLookup {
    id_to_entity_map: HashMap<EntityId, Entity>,
    parent_to_children_map: HashMap<EntityId, Vec<EntityId>>,
    name_to_class_ids_map: HashMap<String, Vec<EntityId>>,
    class_to_bases_map: HashMap<EntityId, Vec<EntityId>>,
    content_to_commit_map: HashMap<ContentId, PseudoCommitId>,
}

impl EntityLookup {
    fn build(entities: &[Entity], deps: &[EntityDep]) -> Self {
        let id_to_entity_map: HashMap<EntityId, Entity> =
            entities.iter().map(|e| (e.id, e.clone())).collect();

        let mut parent_to_children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut name_to_class_ids_map: HashMap<String, Vec<EntityId>> = HashMap::new();

        for e in entities {
            if let Some(pid) = e.parent_id {
                parent_to_children_map.entry(pid).or_default().push(e.id);
            }
            if e.kind == EntityKind::Class {
                name_to_class_ids_map.entry(e.name.clone()).or_default().push(e.id);
            }
        }

        let mut class_to_bases_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        for d in deps {
            if d.kind == DepKind::Extend {
                let src_is_class = id_to_entity_map.get(&d.src).map_or(false, |e| e.kind == EntityKind::Class);
                let tgt_is_class = id_to_entity_map.get(&d.tgt).map_or(false, |e| e.kind == EntityKind::Class);
                if src_is_class && tgt_is_class {
                    class_to_bases_map.entry(d.src).or_default().push(d.tgt);
                }
            }
        }

        let content_to_commit_map: HashMap<ContentId, PseudoCommitId> = deps
            .iter()
            .filter_map(|d| id_to_entity_map.get(&d.src).map(|e| (e.content_id, d.commit_id)))
            .collect();

        Self { id_to_entity_map, parent_to_children_map, name_to_class_ids_map, class_to_bases_map, content_to_commit_map }
    }

    fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.id_to_entity_map.get(&id)
    }

    fn children_of(&self, parent: EntityId) -> &[EntityId] {
        self.parent_to_children_map.get(&parent).map(Vec::as_slice).unwrap_or(&[])
    }

    fn classes_named(&self, name: &str) -> &[EntityId] {
        self.name_to_class_ids_map.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    fn find_inherited_member(
        &self,
        start_class: EntityId,
        kind: EntityKind,
        name: &str,
    ) -> Option<EntityId> {
        let mut visited = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start_class);

        while let Some(class_id) = queue.pop_front() {
            if !visited.insert(class_id) {
                continue;
            }
            for &child in self.children_of(class_id) {
                if let Some(e) = self.entity(child) {
                    if e.kind == kind && e.name == name {
                        return Some(child);
                    }
                }
            }
            if let Some(bases) = self.class_to_bases_map.get(&class_id) {
                queue.extend(bases.iter().copied());
            }
        }
        None
    }

    fn commit_id_for(&self, entity_id: EntityId) -> PseudoCommitId {
        self.id_to_entity_map
            .get(&entity_id)
            .and_then(|e| self.content_to_commit_map.get(&e.content_id))
            .copied()
            .unwrap_or(PseudoCommitId::WorkDir)
    }
}

fn dep_at_row(
    src: EntityId,
    tgt: EntityId,
    kind: DepKind,
    row: usize,
    commit_id: PseudoCommitId,
) -> EntityDep {
    Dep::new(src, tgt, kind, PartialPosition::Whole(Position::new(0, row, 0)), commit_id)
}

#[derive(Debug)]
pub struct PythonDataclassHeuristic;

impl DepEnhancer for PythonDataclassHeuristic {
    fn enhance(
        &self,
        files: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let index = EntityLookup::build(entities, &deps);
        let mut emitted: HashSet<(EntityId, EntityId, DepKind)> =
            deps.iter().map(|d| (d.src, d.tgt, d.kind)).collect();
        let mut result = deps;

        for (filename, content) in files {
            if !filename.ends_with(".py") {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let file_cid = ContentId::from_content(content);

            for e in entities {
                if e.kind != EntityKind::Class || e.content_id != file_cid {
                    continue;
                }
                if !is_dataclass(e, &lines) {
                    continue;
                }

                for &child_id in index.children_of(e.id) {
                    let child = match index.entity(child_id) {
                        Some(c) => c,
                        None => continue,
                    };
                    if child.kind != EntityKind::Field {
                        continue;
                    }

                    let row = child.code.start.row;
                    let line = match lines.get(row) {
                        Some(l) => l,
                        None => continue,
                    };

                    for type_name in extract_type_names(line) {
                        for &class_id in index.classes_named(&type_name) {
                            if class_id == e.id {
                                continue;
                            }
                            if emitted.insert((child_id, class_id, DepKind::Use)) {
                                result.push(dep_at_row(
                                    child_id,
                                    class_id,
                                    DepKind::Use,
                                    row,
                                    index.commit_id_for(child_id),
                                ));
                            }
                        }
                    }
                }
            }
        }

        result
    }
}

fn is_dataclass(class_entity: &Entity, lines: &[&str]) -> bool {
    let start = class_entity.code.start.row;
    let check_from = start.saturating_sub(5);
    for i in (check_from..start).rev() {
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

fn extract_type_names(line: &str) -> Vec<String> {
    let annotation = match line.find(':') {
        Some(i) => &line[i + 1..],
        None => return vec![],
    };
    let annotation = annotation.split('=').next().unwrap_or(annotation);

    let mut names = Vec::new();
    let mut word = String::new();
    for ch in annotation.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            word.push(ch);
        } else {
            if !word.is_empty() {
                let w = std::mem::take(&mut word);
                if w.chars().next().map_or(false, |c| c.is_uppercase()) {
                    names.push(w);
                }
            }
        }
    }
    if !word.is_empty() && word.chars().next().map_or(false, |c| c.is_uppercase()) {
        names.push(word);
    }
    names
}

#[derive(Debug)]
pub struct JavaConstructorHeuristic;

impl DepEnhancer for JavaConstructorHeuristic {
    fn enhance(
        &self,
        files: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let index = EntityLookup::build(entities, &deps);
        let mut emitted: HashSet<(EntityId, EntityId, DepKind)> =
            deps.iter().map(|d| (d.src, d.tgt, d.kind)).collect();
        let mut result = deps;

        for (filename, content) in files {
            if !filename.ends_with(".java") {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let file_cid = ContentId::from_content(content);

            let mut class_to_fields_map: HashMap<EntityId, HashMap<String, EntityId>> = HashMap::new();
            for e in entities {
                if e.content_id != file_cid || e.kind != EntityKind::Field {
                    continue;
                }
                if let Some(pid) = e.parent_id {
                    if index.entity(pid).map_or(false, |p| p.kind == EntityKind::Class) {
                        class_to_fields_map
                            .entry(pid)
                            .or_default()
                            .insert(e.name.clone(), e.id);
                    }
                }
            }

            for e in entities {
                if e.content_id != file_cid || e.kind != EntityKind::Constructor {
                    continue;
                }
                let class_id = match e.parent_id {
                    Some(pid) => pid,
                    None => continue,
                };
                let body_lines =
                    &lines[e.code.start.row.min(lines.len())..e.code.end.row.min(lines.len())];
                let commit_id = index.commit_id_for(e.id);

                if let Some(class_fields) = class_to_fields_map.get(&class_id) {
                    for (line_offset, line) in body_lines.iter().enumerate() {
                        let row = e.code.start.row + line_offset;
                        for field_name in extract_this_field_assignments(line) {
                            if let Some(&field_id) = class_fields.get(&field_name) {
                                if emitted.insert((e.id, field_id, DepKind::Use)) {
                                    result.push(dep_at_row(e.id, field_id, DepKind::Use, row, commit_id));
                                }
                            }
                        }
                    }
                }

                if let Some(bases) = index.class_to_bases_map.get(&class_id) {
                    for &base_id in bases {
                        let base_class_name = index.entity(base_id).map(|e| e.name.as_str()).unwrap_or("");
                        if let Some(ctor_id) = index.find_inherited_member(base_id, EntityKind::Constructor, base_class_name) {
                            if body_lines.iter().any(|l| contains_super_call(l)) {
                                if emitted.insert((e.id, ctor_id, DepKind::Call)) {
                                    result.push(dep_at_row(e.id, ctor_id, DepKind::Call, e.code.start.row, commit_id));
                                }
                            }
                        } else if body_lines.iter().any(|l| contains_super_call(l)) {
                            if emitted.insert((e.id, base_id, DepKind::Call)) {
                                result.push(dep_at_row(e.id, base_id, DepKind::Call, e.code.start.row, commit_id));
                            }
                        }
                    }
                }

                if body_lines.iter().any(|l| contains_this_call(l)) {
                    for &sib_id in index.children_of(class_id) {
                        if sib_id == e.id {
                            continue;
                        }
                        let sib = match index.entity(sib_id) {
                            Some(s) => s,
                            None => continue,
                        };
                        if sib.kind == EntityKind::Constructor {
                            if emitted.insert((e.id, sib_id, DepKind::Call)) {
                                result.push(dep_at_row(e.id, sib_id, DepKind::Call, e.code.start.row, commit_id));
                            }
                        }
                    }
                }
            }
        }

        result
    }
}

fn extract_this_field_assignments(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = line.trim();
    while let Some(pos) = rest.find("this.") {
        rest = &rest[pos + 5..];
        let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
        let name = &rest[..end];
        if name.is_empty() {
            continue;
        }
        let after = rest[end..].trim_start();
        if after.starts_with('=') && !after.starts_with("==") {
            names.push(name.to_string());
        }
        rest = &rest[end..];
    }
    names
}

fn contains_super_call(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("super(") || t.contains(" super(") || t.contains("\tsuper(")
}

fn contains_this_call(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("this(") || t.contains(" this(") || t.contains("\tthis(")
}

#[derive(Debug)]
pub struct JavaOverrideHeuristic;

impl DepEnhancer for JavaOverrideHeuristic {
    fn enhance(
        &self,
        files: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        let index = EntityLookup::build(entities, &deps);
        let mut emitted: HashSet<(EntityId, EntityId, DepKind)> =
            deps.iter().map(|d| (d.src, d.tgt, d.kind)).collect();
        let mut result = deps;

        for (filename, content) in files {
            if !filename.ends_with(".java") {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let file_cid = ContentId::from_content(content);

            for e in entities {
                if e.content_id != file_cid || e.kind != EntityKind::Method {
                    continue;
                }
                let class_id = match e.parent_id {
                    Some(pid) if index.entity(pid).map_or(false, |p| p.kind == EntityKind::Class) => pid,
                    _ => continue,
                };

                if !has_override_annotation(e.code.start.row, &lines) {
                    continue;
                }

                let commit_id = index.commit_id_for(e.id);
                if let Some(bases) = index.class_to_bases_map.get(&class_id) {
                    for &base_id in bases {
                        if let Some(parent_method_id) =
                            index.find_inherited_member(base_id, EntityKind::Method, &e.name)
                        {
                            if emitted.insert((e.id, parent_method_id, DepKind::Override)) {
                                result.push(dep_at_row(
                                    e.id,
                                    parent_method_id,
                                    DepKind::Override,
                                    e.code.start.row,
                                    commit_id,
                                ));
                            }
                            break;
                        }
                    }
                }
            }
        }

        result
    }
}

fn has_override_annotation(method_row: usize, lines: &[&str]) -> bool {
    let check_from = method_row.saturating_sub(5);
    for i in (check_from..method_row).rev() {
        let t = lines[i].trim();
        if t == "@Override" || t.starts_with("@Override ") || t.starts_with("@Override(") {
            return true;
        }
        if !t.is_empty() && !t.starts_with('@') && !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*') {
            break;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{ContentId, EntityKind, Sha1Hash, SimpleEntityId, Span, Position, PartialPosition};

    fn make_sha(n: u8) -> Sha1Hash {
        Sha1Hash::new([n; 20])
    }
    fn make_entity_id(n: u8) -> EntityId {
        crate::core::EntityId(make_sha(n))
    }
    fn make_content_id(n: u8) -> ContentId {
        ContentId(make_sha(n))
    }
    fn make_simple_id(n: u8) -> SimpleEntityId {
        SimpleEntityId(make_sha(n))
    }
    fn span(sr: usize, er: usize) -> Span {
        Span::new(
            Position::new(sr * 10, sr, 0),
            Position::new(er * 10 + 9, er, 79),
        )
    }
    fn entity(id: u8, parent: Option<u8>, name: &str, kind: EntityKind, start_row: usize, end_row: usize, cid: u8) -> Entity {
        Entity {
            id: make_entity_id(id),
            parent_id: parent.map(make_entity_id),
            name: name.to_string(),
            kind,
            code: span(start_row, end_row),
            comment: None,
            content_id: make_content_id(cid),
            simple_id: make_simple_id(id),
        }
    }

    #[test]
    fn chained_enhancer_runs_in_order() {
        #[derive(Debug)]
        struct NoOp;
        impl DepEnhancer for NoOp {
            fn enhance(&self, _f: &HashMap<String, String>, _e: &[Entity], deps: Vec<EntityDep>) -> Vec<EntityDep> { deps }
        }
        let chain = ChainedEnhancer(vec![Arc::new(NoOp), Arc::new(NoOp)]);
        let files = HashMap::new();
        let result = chain.enhance(&files, &[], vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn extract_type_names_finds_uppercase() {
        let names = extract_type_names("    inventory: List[Station]");
        assert!(names.contains(&"List".to_string()));
        assert!(names.contains(&"Station".to_string()));
    }

    #[test]
    fn extract_type_names_skips_lowercase() {
        let names = extract_type_names("    count: int = 0");
        assert!(names.is_empty());
    }

    #[test]
    fn is_dataclass_detects_decorator() {
        let lines = vec!["@dataclass", "class Foo:"];
        let e = entity(1, None, "Foo", EntityKind::Class, 1, 5, 1);
        assert!(is_dataclass(&e, &lines));
    }

    #[test]
    fn is_dataclass_false_without_decorator() {
        let lines = vec!["class Foo:"];
        let e = entity(1, None, "Foo", EntityKind::Class, 0, 5, 1);
        assert!(!is_dataclass(&e, &lines));
    }

    #[test]
    fn extract_this_field_assignments_basic() {
        let names = extract_this_field_assignments("        this.name = value;");
        assert_eq!(names, vec!["name"]);
    }

    #[test]
    fn extract_this_field_assignments_skips_equality() {
        let names = extract_this_field_assignments("        if (this.name == other) {");
        assert!(names.is_empty());
    }

    #[test]
    fn has_override_annotation_detects() {
        let lines = vec!["    @Override", "    public void foo() {"];
        assert!(has_override_annotation(1, &lines));
    }

    #[test]
    fn has_override_annotation_false_when_absent() {
        let lines = vec!["    public void foo() {"];
        assert!(!has_override_annotation(0, &lines));
    }
}
