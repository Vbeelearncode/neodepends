use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

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
use crate::sparse_vec::SparseVec;

pub(crate) struct EntityIndex {
    content_to_byte_owner_map: HashMap<ContentId, SparseVec<EntityId>>,
    file_to_content_id_map: HashMap<String, ContentId>,
    content_to_commit_map: HashMap<ContentId, PseudoCommitId>,
    id_to_entity_map: HashMap<EntityId, Entity>,
    content_to_file_entity_map: HashMap<ContentId, EntityId>,
    pub(crate) name_to_class_ids_map: HashMap<String, Vec<EntityId>>,
    name_to_callable_ids_map: HashMap<String, Vec<EntityId>>,
    pub(crate) class_to_methods_map: HashMap<EntityId, HashMap<String, EntityId>>,
    class_to_fields_map: HashMap<EntityId, HashMap<String, EntityId>>,
    pub(crate) class_to_bases_map: HashMap<EntityId, Vec<EntityId>>,
    pub(crate) class_to_subclasses_map: HashMap<EntityId, Vec<EntityId>>,
    pub(crate) method_to_var_types_map: HashMap<EntityId, HashMap<String, Vec<EntityId>>>,
    parent_to_children_map: HashMap<EntityId, Vec<EntityId>>,
}

impl EntityIndex {
    pub(crate) fn build(
        sources: &HashMap<String, String>,
        entities: &[Entity],
        existing_deps: &[EntityDep],
    ) -> Self {
        let file_to_content_id_map: HashMap<String, ContentId> = sources
            .iter()
            .map(|(filename, content)| (filename.clone(), ContentId::from_content(content)))
            .collect();

        let id_to_entity_map: HashMap<EntityId, &Entity> =
            entities.iter().map(|e| (e.id, e)).collect();

        let content_to_commit_map: HashMap<ContentId, PseudoCommitId> = existing_deps
            .iter()
            .filter_map(|d| id_to_entity_map.get(&d.src).map(|e| (e.content_id, d.commit_id)))
            .collect();

        let depths = compute_nesting_depths(entities);
        let mut entities_shallowest_first: Vec<&Entity> = entities.iter().collect();
        entities_shallowest_first.sort_by_key(|e| depths.get(&e.id).copied().unwrap_or(0));

        let mut content_to_byte_owner_map: HashMap<ContentId, SparseVec<EntityId>> = HashMap::new();
        for entity in &entities_shallowest_first {
            let sparse = content_to_byte_owner_map.entry(entity.content_id).or_insert_with(SparseVec::new);
            if entity.parent_id.is_none() {
                sparse.insert_many(0, usize::MAX, entity.id);
            } else {
                let start = entity.code.start.byte;
                let end = entity.code.end.byte;
                if start <= end {
                    sparse.insert_many(start, end, entity.id);
                }
            }
        }

        let mut content_to_file_entity_map: HashMap<ContentId, EntityId> = HashMap::new();
        let mut name_to_class_ids_map: HashMap<String, Vec<EntityId>> = HashMap::new();
        let mut name_to_callable_ids_map: HashMap<String, Vec<EntityId>> = HashMap::new();
        let mut class_to_methods_map: HashMap<EntityId, HashMap<String, EntityId>> = HashMap::new();
        let mut class_to_fields_map: HashMap<EntityId, HashMap<String, EntityId>> = HashMap::new();
        let mut parent_to_children_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();

        for entity in entities {
            if let Some(pid) = entity.parent_id {
                parent_to_children_map.entry(pid).or_default().push(entity.id);
            }
            match entity.kind {
                EntityKind::File => {
                    content_to_file_entity_map.insert(entity.content_id, entity.id);
                }
                EntityKind::Class => {
                    name_to_class_ids_map.entry(entity.name.clone()).or_default().push(entity.id);
                }
                EntityKind::Method | EntityKind::Function | EntityKind::Constructor => {
                    name_to_callable_ids_map.entry(entity.name.clone()).or_default().push(entity.id);
                    if let Some(parent_id) = entity.parent_id {
                        class_to_methods_map
                            .entry(parent_id)
                            .or_default()
                            .entry(entity.name.clone())
                            .or_insert(entity.id);
                    }
                }
                EntityKind::Field => {
                    if let Some(parent_id) = entity.parent_id {
                        let enclosing_class_id =
                            if id_to_entity_map.get(&parent_id).map_or(false, |p| p.kind == EntityKind::Class) {
                                Some(parent_id)
                            } else {
                                id_to_entity_map
                                    .get(&parent_id)
                                    .and_then(|p| p.parent_id)
                                    .filter(|&gp| {
                                        id_to_entity_map
                                            .get(&gp)
                                            .map_or(false, |e| e.kind == EntityKind::Class)
                                    })
                            };
                        if let Some(class_id) = enclosing_class_id {
                            class_to_fields_map
                                .entry(class_id)
                                .or_default()
                                .entry(entity.name.clone())
                                .or_insert(entity.id);
                        }
                    }
                }
                _ => {}
            }
        }

        let mut class_to_bases_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();
        let mut class_to_subclasses_map: HashMap<EntityId, Vec<EntityId>> = HashMap::new();

        for dep in existing_deps {
            if dep.kind != DepKind::Extend {
                continue;
            }
            let src_is_class = id_to_entity_map.get(&dep.src).map_or(false, |e| e.kind == EntityKind::Class);
            let tgt_is_class = id_to_entity_map.get(&dep.tgt).map_or(false, |e| e.kind == EntityKind::Class);
            if src_is_class && tgt_is_class {
                class_to_bases_map.entry(dep.src).or_default().push(dep.tgt);
                class_to_subclasses_map.entry(dep.tgt).or_default().push(dep.src);
            }
        }

        EntityIndex {
            content_to_byte_owner_map,
            file_to_content_id_map,
            content_to_commit_map,
            id_to_entity_map: entities.iter().map(|e| (e.id, e.clone())).collect(),
            content_to_file_entity_map,
            name_to_class_ids_map,
            name_to_callable_ids_map,
            class_to_methods_map,
            class_to_fields_map,
            class_to_bases_map,
            class_to_subclasses_map,
            method_to_var_types_map: HashMap::new(),
            parent_to_children_map,
        }
    }

    pub(crate) fn add_base_class(&mut self, subclass_id: EntityId, base_id: EntityId) {
        let bases = self.class_to_bases_map.entry(subclass_id).or_default();
        if !bases.contains(&base_id) {
            bases.push(base_id);
            self.class_to_subclasses_map.entry(base_id).or_default().push(subclass_id);
        }
    }

    pub(crate) fn add_var_type(&mut self, method_id: EntityId, var_name: String, class_id: EntityId) {
        self.method_to_var_types_map
            .entry(method_id)
            .or_default()
            .entry(var_name)
            .or_default()
            .push(class_id);
    }

    pub(crate) fn owner_at(&self, filename: &str, byte: usize) -> Option<EntityId> {
        let cid = self.file_to_content_id_map.get(filename)?;
        self.content_to_byte_owner_map.get(cid)?.get(byte)
    }

    pub(crate) fn content_id_of_file(&self, filename: &str) -> Option<ContentId> {
        self.file_to_content_id_map.get(filename).copied()
    }

    pub(crate) fn commit_id_of_entity(&self, entity_id: EntityId) -> PseudoCommitId {
        self.id_to_entity_map
            .get(&entity_id)
            .and_then(|e| self.content_to_commit_map.get(&e.content_id))
            .copied()
            .unwrap_or(PseudoCommitId::WorkDir)
    }

    pub(crate) fn entity(&self, id: EntityId) -> Option<&Entity> {
        self.id_to_entity_map.get(&id)
    }

    pub(crate) fn file_entity_for(&self, filename: &str) -> Option<EntityId> {
        let cid = self.file_to_content_id_map.get(filename)?;
        self.content_to_file_entity_map.get(cid).copied()
    }

    pub(crate) fn bases_of(&self, class_id: EntityId) -> Option<&[EntityId]> {
        self.class_to_bases_map.get(&class_id).map(Vec::as_slice)
    }

    pub(crate) fn children_of(&self, parent_id: EntityId) -> &[EntityId] {
        self.parent_to_children_map.get(&parent_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub(crate) fn classes_named(&self, name: &str) -> &[EntityId] {
        self.name_to_class_ids_map.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    pub(crate) fn enclosing_callable_of(&self, entity_id: EntityId) -> Option<EntityId> {
        let mut cur = entity_id;
        loop {
            let e = self.id_to_entity_map.get(&cur)?;
            let parent_id = e.parent_id?;
            let parent = self.id_to_entity_map.get(&parent_id)?;
            if matches!(parent.kind, EntityKind::Method | EntityKind::Function | EntityKind::Constructor) {
                return Some(parent_id);
            }
            if matches!(parent.kind, EntityKind::Class | EntityKind::File) {
                return None;
            }
            cur = parent_id;
        }
    }

    pub(crate) fn enclosing_class_of(&self, entity_id: EntityId) -> Option<EntityId> {
        let entity = self.id_to_entity_map.get(&entity_id)?;
        if entity.kind == EntityKind::Class {
            return None;
        }
        let mut cur = entity_id;
        loop {
            let e = self.id_to_entity_map.get(&cur)?;
            let parent_id = e.parent_id?;
            let parent = self.id_to_entity_map.get(&parent_id)?;
            if parent.kind == EntityKind::Class {
                return Some(parent_id);
            }
            cur = parent_id;
        }
    }

    pub(crate) fn resolve_field(&self, start_class: EntityId, name: &str) -> Option<EntityId> {
        self.bfs_class_hierarchy(start_class, |class_id| {
            self.class_to_fields_map.get(&class_id).and_then(|m| m.get(name)).copied()
        })
    }

    pub(crate) fn resolve_method(&self, start_class: EntityId, name: &str) -> Option<EntityId> {
        self.bfs_class_hierarchy(start_class, |class_id| {
            self.class_to_methods_map.get(&class_id).and_then(|m| m.get(name)).copied()
        })
    }

    pub(crate) fn find_inherited_member(&self, start_class: EntityId, kind: EntityKind, name: &str) -> Option<EntityId> {
        self.bfs_class_hierarchy(start_class, |class_id| {
            self.children_of(class_id)
                .iter()
                .find(|&&child_id| {
                    self.id_to_entity_map.get(&child_id)
                        .map_or(false, |e| e.kind == kind && e.name == name)
                })
                .copied()
        })
    }

    fn bfs_class_hierarchy<F>(&self, start_class: EntityId, find_in_class: F) -> Option<EntityId>
    where
        F: Fn(EntityId) -> Option<EntityId>,
    {
        let mut visited: HashSet<EntityId> = HashSet::new();
        let mut queue: VecDeque<EntityId> = VecDeque::new();
        queue.push_back(start_class);
        while let Some(class_id) = queue.pop_front() {
            if !visited.insert(class_id) {
                continue;
            }
            if let Some(found) = find_in_class(class_id) {
                return Some(found);
            }
            if let Some(bases) = self.class_to_bases_map.get(&class_id) {
                queue.extend(bases.iter().copied());
            }
        }
        None
    }

    pub(crate) fn is_package_init_file(&self, entity_id: EntityId) -> bool {
        self.id_to_entity_map.get(&entity_id).map_or(false, |e| {
            e.kind == EntityKind::File
                && (e.name == "__init__.py" || e.name.ends_with("/__init__.py"))
        })
    }

    pub(crate) fn resolve_class(&self, name: &str, preferred_content_id: ContentId) -> Option<EntityId> {
        let classes = self.name_to_class_ids_map.get(name)?;
        if classes.len() == 1 {
            return Some(classes[0]);
        }
        classes
            .iter()
            .find(|&&id| {
                self.id_to_entity_map
                    .get(&id)
                    .map_or(false, |e| e.content_id == preferred_content_id)
            })
            .copied()
            .or_else(|| classes.first().copied())
    }

    pub(crate) fn resolve_callable(&self, name: &str, preferred_content_id: ContentId) -> Option<EntityId> {
        let callables = self.name_to_callable_ids_map.get(name)?;
        if callables.len() == 1 {
            return Some(callables[0]);
        }
        callables
            .iter()
            .find(|&&id| {
                self.id_to_entity_map
                    .get(&id)
                    .map_or(false, |e| e.content_id == preferred_content_id)
            })
            .copied()
            .or_else(|| callables.first().copied())
    }
}

fn compute_nesting_depths(entities: &[Entity]) -> HashMap<EntityId, usize> {
    let all_ids: HashSet<EntityId> = entities.iter().map(|e| e.id).collect();
    let mut depths: HashMap<EntityId, usize> = HashMap::with_capacity(entities.len());

    for e in entities {
        if e.parent_id.is_none() || e.parent_id.map_or(true, |p| !all_ids.contains(&p)) {
            depths.insert(e.id, 0);
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for e in entities {
            if depths.contains_key(&e.id) {
                continue;
            }
            if let Some(parent_id) = e.parent_id {
                if let Some(&parent_depth) = depths.get(&parent_id) {
                    depths.insert(e.id, parent_depth + 1);
                    changed = true;
                }
            }
        }
    }

    for e in entities {
        depths.entry(e.id).or_insert(0);
    }

    depths
}

pub(crate) fn dep_at_position(
    src: EntityId,
    tgt: EntityId,
    kind: crate::core::DepKind,
    byte: usize,
    row: usize,
    col: usize,
    commit_id: PseudoCommitId,
) -> EntityDep {
    Dep::new(src, tgt, kind, PartialPosition::Whole(Position::new(byte, row, col)), commit_id)
}

pub(crate) fn dep_at_row(
    src: EntityId,
    tgt: EntityId,
    kind: crate::core::DepKind,
    row: usize,
    commit_id: PseudoCommitId,
) -> EntityDep {
    Dep::new(src, tgt, kind, PartialPosition::Whole(Position::new(0, row, 0)), commit_id)
}

pub(crate) fn dedup_edges(deps: &mut Vec<EntityDep>) {
    let mut seen: HashSet<(EntityId, EntityId, crate::core::DepKind)> = HashSet::new();
    deps.retain(|d| seen.insert((d.src, d.tgt, d.kind)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{SimpleEntityId, Span};

    fn make_entity(
        name: &str,
        kind: EntityKind,
        parent_id: Option<EntityId>,
        content_id: ContentId,
        start_byte: usize,
        end_byte: usize,
    ) -> Entity {
        let code = Span::new(
            Position::new(start_byte, 0, 0),
            Position::new(end_byte, 0, 0),
        );
        let simple_id = SimpleEntityId::new(None, name, kind);
        Entity::new(parent_id, name.to_string(), kind, code, None, content_id, simple_id)
    }

    #[test]
    fn resolve_field_walks_inheritance() {
        let content_base = "class Base:\n    def __init__(self): self.val = 0\n";
        let content_child = "class Child(Base): pass\n";
        let cid_base = ContentId::from_content(content_base);
        let cid_child = ContentId::from_content(content_child);

        let sources = std::collections::HashMap::from([
            ("base.py".to_string(), content_base.to_string()),
            ("child.py".to_string(), content_child.to_string()),
        ]);

        let base_file = make_entity("base.py", EntityKind::File, None, cid_base, 0, content_base.len());
        let base_class = make_entity("Base", EntityKind::Class, Some(base_file.id), cid_base, 0, content_base.len());
        let init_method = make_entity("__init__", EntityKind::Method, Some(base_class.id), cid_base, 12, content_base.len());
        let val_field = make_entity("val", EntityKind::Field, Some(init_method.id), cid_base, 38, 44);

        let child_file = make_entity("child.py", EntityKind::File, None, cid_child, 0, content_child.len());
        let child_class = make_entity("Child", EntityKind::Class, Some(child_file.id), cid_child, 0, content_child.len());

        let entities = vec![base_file, base_class.clone(), init_method, val_field.clone(), child_file, child_class.clone()];

        let extend_dep = Dep::new(
            child_class.id, base_class.id, DepKind::Extend,
            PartialPosition::Row(0), PseudoCommitId::WorkDir,
        );
        let index = EntityIndex::build(&sources, &entities, &[extend_dep]);

        assert_eq!(
            index.resolve_field(child_class.id, "val"),
            Some(val_field.id),
            "should resolve inherited field through base class chain"
        );
    }
}
