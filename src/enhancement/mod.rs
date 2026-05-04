use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use crate::core::Entity;
use crate::core::EntityDep;

pub mod entity_index;
pub mod java;
pub mod python;
pub mod returns;

pub use java::{JavaConstructorHeuristic, JavaOverrideHeuristic};
pub use python::{PythonDataclassHeuristic, PythonQueryEnhancer};


pub trait DepEnhancer: Debug + Send + Sync {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[Entity],
        deps: Vec<EntityDep>,
    ) -> Vec<EntityDep>;
}

#[derive(Debug)]
pub struct ChainedEnhancer(pub Vec<Arc<dyn DepEnhancer>>);

impl DepEnhancer for ChainedEnhancer {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[Entity],
        mut deps: Vec<EntityDep>,
    ) -> Vec<EntityDep> {
        for enhancer in &self.0 {
            deps = enhancer.enhance(sources, entities, deps);
        }
        deps
    }
}
