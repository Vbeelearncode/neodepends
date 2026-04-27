use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

use itertools::Itertools;

use crate::core::Change;
use crate::core::Entity;
use crate::core::EntityDep;
use crate::core::EntityId;
use crate::core::EntityKind;

pub fn dsm_v1(entities: &[Entity], deps: &[EntityDep], changes: &[Change]) -> String {
    if entities.iter().any(|e| !e.kind.is_file()) {
        panic!("DSMv1 can only be made with files");
    }

    if entities.len() != entities.iter().map(|e| &e.name).unique().count() {
        panic!("DSMv1 must have unique filenames");
    }

    let indices: HashMap<_, _> = entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();

    let cochanges = calc_cochanges(&entities, &changes)
        .into_iter()
        .map(|(a, b)| ((indices[&a], indices[&b]), "Cochange"));

    let cells = deps
        .iter()
        .map(|d| ((indices[&d.src], indices[&d.tgt]), d.kind.as_ref()))
        .chain(cochanges)
        .into_group_map()
        .into_iter()
        .map(|((src, tgt), kinds)| CellV1::new(src, tgt, kinds))
        .sorted_by_key(|c| c.as_pair())
        .collect();

    let variables = entities.into_iter().map(|e| &e.name).collect();
    let matrix = Matrix { schema: "1.0".to_string(), variables, cells };
    serde_json::to_string_pretty(&matrix).unwrap()
}

fn dedup_by_display_path<'a>(
    entities: &'a [Entity],
    id_to_entity_map: &HashMap<EntityId, &Entity>,
) -> (Vec<&'a Entity>, HashMap<EntityId, EntityId>) {
    let mut path_to_first_id_map: HashMap<String, EntityId> = HashMap::new();
    let mut id_to_canonical_id_map: HashMap<EntityId, EntityId> = HashMap::new();
    for entity in entities {
        let path = entity_display_path(entity, id_to_entity_map);
        let canonical = *path_to_first_id_map.entry(path).or_insert(entity.id);
        id_to_canonical_id_map.insert(entity.id, canonical);
    }
    let canonical_entities =
        entities.iter().filter(|e| id_to_canonical_id_map[&e.id] == e.id).collect();
    (canonical_entities, id_to_canonical_id_map)
}

pub fn dsm_v2(entities: &[Entity], deps: &[EntityDep], changes: &[Change]) -> String {
    let id_to_entity_map: HashMap<EntityId, &Entity> = entities.iter().map(|e| (e.id, e)).collect();
    let (canonical_entities, id_to_canonical_id_map) =
        dedup_by_display_path(entities, &id_to_entity_map);
    let id_to_index_map: HashMap<EntityId, usize> =
        canonical_entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
    let canonical_idx = |id: EntityId| id_to_index_map[&id_to_canonical_id_map[&id]];

    let cochanges = calc_cochanges(entities, changes)
        .into_iter()
        .map(|(a, b)| ((canonical_idx(a), canonical_idx(b)), "Cochange"))
        .filter(|((src, tgt), _)| src != tgt);

    let cells: Vec<CellV1> = deps
        .iter()
        .map(|d| ((canonical_idx(d.src), canonical_idx(d.tgt)), d.kind.as_ref()))
        .unique()
        .filter(|((src, tgt), _)| src != tgt)
        .chain(cochanges)
        .into_group_map()
        .into_iter()
        .map(|((src, tgt), kinds)| CellV1::new(src, tgt, kinds))
        .sorted_by_key(|c| c.as_pair())
        .collect();

    let variables: Vec<String> =
        canonical_entities.iter().map(|e| entity_display_path(e, &id_to_entity_map)).collect();
    serde_json::to_string_pretty(&Matrix { schema: "2.0".to_string(), variables, cells }).unwrap()
}

#[derive(Debug, Clone)]
#[derive(serde::Serialize)]
struct Matrix<V, C> {
    schema: String,
    variables: Vec<V>,
    cells: Vec<C>,
}

fn entity_base_path(entity: &Entity, id_to_entity_map: &HashMap<EntityId, &Entity>) -> String {
    match entity.parent_id {
        None => entity.name.clone(),
        Some(pid) => format!(
            "{}/{}",
            entity_base_path(id_to_entity_map[&pid], id_to_entity_map),
            entity.name
        ),
    }
}

fn leaf_kind_subfolder(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Method => "methods",
        EntityKind::Constructor => "constructors",
        EntityKind::Function => "functions",
        EntityKind::Annotation => "annotations",
        _ => unreachable!(),
    }
}

fn field_owner<'a>(
    entity: &Entity,
    id_to_entity_map: &HashMap<EntityId, &'a Entity>,
) -> &'a Entity {
    let parent = id_to_entity_map[&entity.parent_id.unwrap()];
    match parent.kind {
        EntityKind::Method | EntityKind::Constructor | EntityKind::Function => {
            field_owner(parent, id_to_entity_map)
        }
        _ => parent,
    }
}

fn entity_display_path(entity: &Entity, id_to_entity_map: &HashMap<EntityId, &Entity>) -> String {
    let kind = entity.kind.as_ref();
    match entity.kind {
        EntityKind::File
        | EntityKind::Class
        | EntityKind::Enum
        | EntityKind::Interface
        | EntityKind::Record => {
            format!("{}/self ({kind})", entity_base_path(entity, id_to_entity_map))
        }
        EntityKind::Field => {
            let owner_base =
                entity_base_path(field_owner(entity, id_to_entity_map), id_to_entity_map);
            format!("{owner_base}/fields/{} ({kind})", entity.name)
        }
        _ => {
            let parent_base =
                entity_base_path(id_to_entity_map[&entity.parent_id.unwrap()], id_to_entity_map);
            format!("{parent_base}/{}/{} ({kind})", leaf_kind_subfolder(entity.kind), entity.name)
        }
    }
}

#[derive(Debug, Clone)]
#[derive(serde::Serialize)]
struct CellV1 {
    src: usize,
    #[serde(rename = "dest")]
    tgt: usize,
    values: BTreeMap<String, f64>,
}

impl CellV1 {
    fn new(src: usize, tgt: usize, kinds: Vec<&str>) -> Self {
        let values = to_cell_values(kinds).into_iter().map(|(k, c)| (k, c as f64)).collect();
        Self { src, tgt, values }
    }

    fn as_pair(&self) -> (usize, usize) {
        (self.src, self.tgt)
    }
}

fn to_cell_values(kinds: Vec<&str>) -> BTreeMap<String, usize> {
    kinds.into_iter().counts().into_iter().sorted().map(|(k, c)| (k.to_string(), c)).collect()
}

fn calc_cochanges(entities: &[Entity], changes: &[Change]) -> Vec<(EntityId, EntityId)> {
    let id_map = entities.iter().map(|e| (e.simple_id, e.id)).into_group_map();

    let commits = changes
        .iter()
        .map(|c| (c.simple_id, c.commit_id))
        .unique()
        .filter_map(|(s, c)| id_map.get(&s).map(|es| (c, es)))
        .flat_map(|(c, es)| es.iter().map(move |&e| (e, c)))
        .into_grouping_map()
        .collect::<HashSet<_>>();

    let mut pairs = Vec::new();
    let entity_ids = commits.keys().collect_vec();

    for i in 0..entity_ids.len() {
        let i_id = entity_ids[i];
        let i_commits = &commits[i_id];
        for j in (i + 1)..entity_ids.len() {
            let j_id = entity_ids[j];
            let j_commits = &commits[j_id];

            for _ in i_commits.intersection(j_commits) {
                pairs.push((*i_id, *j_id));
                pairs.push((*j_id, *i_id));
            }
        }
    }

    pairs
}
