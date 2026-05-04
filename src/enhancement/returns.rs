#![allow(dead_code)]

use std::collections::BTreeSet;
use std::collections::HashMap;

use crate::core::EntityId;
use crate::core::Position;
use crate::enhancement::entity_index::EntityIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TypeConfidence {
    Low,
    Medium,
    High,
}

impl TypeConfidence {
    fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TypeEvidenceSource {
    ReturnAnnotation,
    ReturnConstructor,
    ReturnVariable,
    ReturnField,
    ReturnCall,
    AssignmentConstructor,
    FieldType,
    ReceiverType,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypeEvidence {
    pub(crate) confidence: TypeConfidence,
    pub(crate) source: TypeEvidenceSource,
}

impl TypeEvidence {
    pub(crate) fn high(source: TypeEvidenceSource) -> Self {
        Self { confidence: TypeConfidence::High, source }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InferredType {
    pub(crate) type_id: EntityId,
    pub(crate) confidence: TypeConfidence,
    pub(crate) evidence: BTreeSet<TypeEvidence>,
}

impl InferredType {
    pub(crate) fn new(type_id: EntityId, evidence: TypeEvidence) -> Self {
        let confidence = evidence.confidence;
        Self { type_id, confidence, evidence: BTreeSet::from([evidence]) }
    }

    fn merge(&mut self, other: &InferredType) -> bool {
        let old_confidence = self.confidence;
        let old_evidence_len = self.evidence.len();
        if other.confidence > self.confidence {
            self.confidence = other.confidence;
        }
        self.evidence.extend(other.evidence.iter().cloned());
        self.confidence != old_confidence || self.evidence.len() != old_evidence_len
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ReturnSummary {
    returns: HashMap<EntityId, InferredType>,
}

impl ReturnSummary {
    pub(crate) fn returns(&self) -> impl Iterator<Item = &InferredType> {
        self.returns.values()
    }

    fn insert(&mut self, inferred: InferredType) -> bool {
        if let Some(existing) = self.returns.get_mut(&inferred.type_id) {
            existing.merge(&inferred)
        } else {
            self.returns.insert(inferred.type_id, inferred);
            true
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ReturnExpr {
    Type(EntityId),
    Variable(String),
    Field { class_id: EntityId, field_name: String },
    Call(EntityId),
    MethodCall { receiver: Box<ReturnExpr>, method_name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReturnFact {
    pub(crate) callable_id: EntityId,
    pub(crate) expr: ReturnExpr,
    pub(crate) evidence: TypeEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AssignmentFact {
    pub(crate) scope_id: EntityId,
    pub(crate) variable: String,
    pub(crate) expr: ReturnExpr,
    pub(crate) evidence: TypeEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FieldFact {
    pub(crate) class_id: EntityId,
    pub(crate) field_name: String,
    pub(crate) type_id: EntityId,
    pub(crate) evidence: TypeEvidence,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ReturnFacts {
    pub(crate) returns: Vec<ReturnFact>,
    pub(crate) assignments: Vec<AssignmentFact>,
    pub(crate) fields: Vec<FieldFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReceiverCallFact {
    pub(crate) caller_id: EntityId,
    pub(crate) receiver: ReturnExpr,
    pub(crate) method_name: String,
    pub(crate) position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceiverCallCandidate {
    pub(crate) caller_id: EntityId,
    pub(crate) callee_id: EntityId,
    pub(crate) receiver_type_id: EntityId,
    pub(crate) confidence: TypeConfidence,
    pub(crate) evidence: BTreeSet<TypeEvidence>,
    pub(crate) polymorphic: bool,
    pub(crate) position: Position,
}

pub(crate) trait TypeMemberResolver {
    fn resolve_method(&self, type_id: EntityId, method_name: &str) -> Option<EntityId>;
}

impl TypeMemberResolver for EntityIndex {
    fn resolve_method(&self, type_id: EntityId, method_name: &str) -> Option<EntityId> {
        EntityIndex::resolve_method(self, type_id, method_name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReturnSolverOptions {
    pub(crate) max_iterations: usize,
    pub(crate) min_confidence: TypeConfidence,
}

impl Default for ReturnSolverOptions {
    fn default() -> Self {
        Self { max_iterations: 32, min_confidence: TypeConfidence::High }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ReturnSolverResult {
    pub(crate) summaries: HashMap<EntityId, ReturnSummary>,
    pub(crate) variable_summaries: HashMap<(EntityId, String), ReturnSummary>,
    pub(crate) field_summaries: HashMap<(EntityId, String), ReturnSummary>,
    pub(crate) stabilized: bool,
    pub(crate) iterations: usize,
}

impl ReturnSolverResult {
    pub(crate) fn resolve_receiver_calls(
        &self,
        calls: &[ReceiverCallFact],
        resolver: &dyn TypeMemberResolver,
        min_confidence: TypeConfidence,
    ) -> Vec<ReceiverCallCandidate> {
        calls
            .iter()
            .flat_map(|call| self.resolve_one_receiver_call(call, resolver, min_confidence))
            .collect()
    }

    fn resolve_one_receiver_call(
        &self,
        call: &ReceiverCallFact,
        resolver: &dyn TypeMemberResolver,
        min_confidence: TypeConfidence,
    ) -> Vec<ReceiverCallCandidate> {
        let receiver_types = self.receiver_types(call, resolver);
        let mut candidates = receiver_types
            .into_iter()
            .filter(|receiver_type| receiver_type.confidence >= min_confidence)
            .filter_map(|receiver_type| receiver_candidate(call, receiver_type, resolver))
            .collect::<Vec<_>>();
        mark_polymorphic(&mut candidates);
        candidates
    }

    fn receiver_types(&self, call: &ReceiverCallFact, resolver: &dyn TypeMemberResolver) -> Vec<InferredType> {
        infer_expr(
            call.caller_id,
            &call.receiver,
            &TypeEvidence::high(TypeEvidenceSource::ReceiverType),
            &self.summaries,
            &self.variable_summaries,
            &self.field_summaries,
            Some(resolver),
        )
    }
}

pub(crate) struct ReturnSolver;

impl ReturnSolver {
    pub(crate) fn solve(facts: &ReturnFacts, options: ReturnSolverOptions) -> ReturnSolverResult {
        Self::solve_with_member_resolver(facts, options, None)
    }

    pub(crate) fn solve_with_member_resolver(
        facts: &ReturnFacts,
        options: ReturnSolverOptions,
        member_resolver: Option<&dyn TypeMemberResolver>,
    ) -> ReturnSolverResult {
        let mut state = ReturnSolverState::new(facts);
        let mut stabilized = false;
        let mut iterations = 0;

        for iteration in 0..options.max_iterations {
            iterations = iteration + 1;
            if !state.apply_facts(facts, options.min_confidence, member_resolver) {
                stabilized = true;
                break;
            }
        }

        state.into_result(stabilized, iterations)
    }
}

struct ReturnSolverState {
    summaries: HashMap<EntityId, ReturnSummary>,
    variable_summaries: HashMap<(EntityId, String), ReturnSummary>,
    field_summaries: HashMap<(EntityId, String), ReturnSummary>,
}

impl ReturnSolverState {
    fn new(facts: &ReturnFacts) -> Self {
        Self {
            summaries: HashMap::new(),
            variable_summaries: HashMap::new(),
            field_summaries: field_summaries_by_class_and_name(&facts.fields),
        }
    }

    fn apply_facts(&mut self, facts: &ReturnFacts, min_confidence: TypeConfidence, resolver: Option<&dyn TypeMemberResolver>) -> bool {
        self.apply_assignment_facts(&facts.assignments, min_confidence, resolver)
            | self.apply_return_facts(&facts.returns, min_confidence, resolver)
    }

    fn apply_assignment_facts(&mut self, facts: &[AssignmentFact], min_confidence: TypeConfidence, resolver: Option<&dyn TypeMemberResolver>) -> bool {
        let mut changed = false;
        for fact in facts {
            let inferred = self.infer(&fact.expr, fact.scope_id, &fact.evidence, resolver);
            let summary = self.variable_summaries.entry((fact.scope_id, fact.variable.clone())).or_default();
            changed |= insert_above_threshold(summary, inferred, min_confidence);
        }
        changed
    }

    fn apply_return_facts(&mut self, facts: &[ReturnFact], min_confidence: TypeConfidence, resolver: Option<&dyn TypeMemberResolver>) -> bool {
        let mut changed = false;
        for fact in facts {
            let inferred = self.infer(&fact.expr, fact.callable_id, &fact.evidence, resolver);
            let summary = self.summaries.entry(fact.callable_id).or_default();
            changed |= insert_above_threshold(summary, inferred, min_confidence);
        }
        changed
    }

    fn infer(&self, expr: &ReturnExpr, scope_id: EntityId, evidence: &TypeEvidence, resolver: Option<&dyn TypeMemberResolver>) -> Vec<InferredType> {
        infer_expr(scope_id, expr, evidence, &self.summaries, &self.variable_summaries, &self.field_summaries, resolver)
    }

    fn into_result(self, stabilized: bool, iterations: usize) -> ReturnSolverResult {
        ReturnSolverResult {
            summaries: self.summaries,
            variable_summaries: self.variable_summaries,
            field_summaries: self.field_summaries,
            stabilized,
            iterations,
        }
    }
}

fn field_summaries_by_class_and_name(fields: &[FieldFact]) -> HashMap<(EntityId, String), ReturnSummary> {
    let mut result: HashMap<(EntityId, String), ReturnSummary> = HashMap::new();
    for field in fields {
        result
            .entry((field.class_id, field.field_name.clone()))
            .or_default()
            .insert(InferredType::new(field.type_id, field.evidence.clone()));
    }
    result
}

fn infer_expr(
    scope_id: EntityId,
    expr: &ReturnExpr,
    evidence: &TypeEvidence,
    summaries: &HashMap<EntityId, ReturnSummary>,
    variable_summaries: &HashMap<(EntityId, String), ReturnSummary>,
    field_summaries: &HashMap<(EntityId, String), ReturnSummary>,
    member_resolver: Option<&dyn TypeMemberResolver>,
) -> Vec<InferredType> {
    match expr {
        ReturnExpr::Type(type_id) => vec![InferredType::new(*type_id, evidence.clone())],
        ReturnExpr::Variable(name) => infer_variable(scope_id, name, evidence, variable_summaries),
        ReturnExpr::Field { class_id, field_name } => infer_field(*class_id, field_name, evidence, field_summaries),
        ReturnExpr::Call(callee_id) => infer_call(*callee_id, evidence, summaries),
        ReturnExpr::MethodCall { receiver, method_name } =>
            infer_method_call(scope_id, receiver, method_name, evidence, summaries, variable_summaries, field_summaries, member_resolver),
    }
}

fn infer_variable(
    scope_id: EntityId,
    name: &str,
    evidence: &TypeEvidence,
    variable_summaries: &HashMap<(EntityId, String), ReturnSummary>,
) -> Vec<InferredType> {
    variable_summaries
        .get(&(scope_id, name.to_string()))
        .into_iter()
        .flat_map(|summary| summary.returns())
        .map(|inferred| derive_from(inferred, evidence, TypeEvidenceSource::ReturnVariable))
        .collect()
}

fn infer_field(
    class_id: EntityId,
    field_name: &str,
    evidence: &TypeEvidence,
    field_summaries: &HashMap<(EntityId, String), ReturnSummary>,
) -> Vec<InferredType> {
    field_summaries
        .get(&(class_id, field_name.to_string()))
        .into_iter()
        .flat_map(|summary| summary.returns())
        .map(|inferred| derive_from(inferred, evidence, TypeEvidenceSource::ReturnField))
        .collect()
}

fn infer_call(callee_id: EntityId, evidence: &TypeEvidence, summaries: &HashMap<EntityId, ReturnSummary>) -> Vec<InferredType> {
    summaries
        .get(&callee_id)
        .into_iter()
        .flat_map(|summary| summary.returns())
        .map(|inferred| derive_from(inferred, evidence, TypeEvidenceSource::ReturnCall))
        .collect()
}

fn infer_method_call(
    scope_id: EntityId,
    receiver: &ReturnExpr,
    method_name: &str,
    evidence: &TypeEvidence,
    summaries: &HashMap<EntityId, ReturnSummary>,
    variable_summaries: &HashMap<(EntityId, String), ReturnSummary>,
    field_summaries: &HashMap<(EntityId, String), ReturnSummary>,
    member_resolver: Option<&dyn TypeMemberResolver>,
) -> Vec<InferredType> {
    let Some(member_resolver) = member_resolver else { return vec![] };
    infer_expr(scope_id, receiver, evidence, summaries, variable_summaries, field_summaries, Some(member_resolver))
        .into_iter()
        .flat_map(|receiver_type| infer_resolved_method(receiver_type, method_name, evidence, summaries, member_resolver))
        .collect()
}

fn infer_resolved_method(
    receiver_type: InferredType,
    method_name: &str,
    evidence: &TypeEvidence,
    summaries: &HashMap<EntityId, ReturnSummary>,
    member_resolver: &dyn TypeMemberResolver,
) -> Vec<InferredType> {
    let Some(callee_id) = member_resolver.resolve_method(receiver_type.type_id, method_name) else { return vec![] };
    infer_call(callee_id, evidence, summaries)
}

fn insert_above_threshold(summary: &mut ReturnSummary, inferred: Vec<InferredType>, min_confidence: TypeConfidence) -> bool {
    inferred
        .into_iter()
        .filter(|item| item.confidence >= min_confidence)
        .fold(false, |changed, item| summary.insert(item) | changed)
}

fn receiver_candidate(
    call: &ReceiverCallFact,
    receiver_type: InferredType,
    resolver: &dyn TypeMemberResolver,
) -> Option<ReceiverCallCandidate> {
    Some(ReceiverCallCandidate {
        caller_id: call.caller_id,
        callee_id: resolver.resolve_method(receiver_type.type_id, &call.method_name)?,
        receiver_type_id: receiver_type.type_id,
        confidence: receiver_type.confidence,
        evidence: receiver_type.evidence,
        polymorphic: false,
        position: call.position,
    })
}

fn mark_polymorphic(candidates: &mut [ReceiverCallCandidate]) {
    let polymorphic = candidates.len() > 1;
    for candidate in candidates {
        candidate.polymorphic = polymorphic;
    }
}

fn derive_from(
    inferred: &InferredType,
    return_evidence: &TypeEvidence,
    source: TypeEvidenceSource,
) -> InferredType {
    let confidence = inferred.confidence.min(return_evidence.confidence);
    let mut evidence = inferred.evidence.clone();
    evidence.insert(return_evidence.clone());
    evidence.insert(TypeEvidence { confidence, source });
    InferredType { type_id: inferred.type_id, confidence, evidence }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Sha1Hash;
    use std::collections::HashMap;

    fn id(n: u8) -> EntityId {
        EntityId(Sha1Hash::hash(&[n]))
    }

    fn returns(summary: &ReturnSummary) -> BTreeSet<EntityId> {
        summary.returns().map(|r| r.type_id).collect()
    }

    #[derive(Default)]
    struct FakeMemberResolver {
        methods: HashMap<(EntityId, String), EntityId>,
    }

    impl FakeMemberResolver {
        fn with_method(mut self, class_id: EntityId, method_name: &str, method_id: EntityId) -> Self {
            self.methods.insert((class_id, method_name.to_string()), method_id);
            self
        }
    }

    impl TypeMemberResolver for FakeMemberResolver {
        fn resolve_method(&self, type_id: EntityId, method_name: &str) -> Option<EntityId> {
            self.methods.get(&(type_id, method_name.to_string())).copied()
        }
    }

    #[test]
    fn keeps_multiple_direct_return_types() {
        let callable = id(1);
        let admin = id(2);
        let guest = id(3);
        let facts = ReturnFacts {
            returns: vec![
                ReturnFact {
                    callable_id: callable,
                    expr: ReturnExpr::Type(admin),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
                },
                ReturnFact {
                    callable_id: callable,
                    expr: ReturnExpr::Type(guest),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
                },
            ],
            ..Default::default()
        };

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());

        assert!(result.stabilized);
        assert_eq!(returns(result.summaries.get(&callable).unwrap()), BTreeSet::from([admin, guest]));
    }

    #[test]
    fn propagates_return_types_through_calls_to_fixed_point() {
        let a = id(1);
        let b = id(2);
        let user = id(3);
        let facts = ReturnFacts {
            returns: vec![
                ReturnFact {
                    callable_id: a,
                    expr: ReturnExpr::Call(b),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnCall),
                },
                ReturnFact {
                    callable_id: b,
                    expr: ReturnExpr::Type(user),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
                },
            ],
            ..Default::default()
        };

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());

        assert!(result.stabilized);
        assert_eq!(returns(result.summaries.get(&a).unwrap()), BTreeSet::from([user]));
        assert!(result.iterations > 1);
    }

    #[test]
    fn resolves_variable_returns_from_assignment_facts() {
        let callable = id(1);
        let user = id(2);
        let facts = ReturnFacts {
            returns: vec![ReturnFact {
                callable_id: callable,
                expr: ReturnExpr::Variable("svc".to_string()),
                evidence: TypeEvidence::high(TypeEvidenceSource::ReturnVariable),
            }],
            assignments: vec![AssignmentFact {
                scope_id: callable,
                variable: "svc".to_string(),
                expr: ReturnExpr::Type(user),
                evidence: TypeEvidence::high(TypeEvidenceSource::AssignmentConstructor),
            }],
            ..Default::default()
        };

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());

        assert!(result.stabilized);
        assert_eq!(returns(result.summaries.get(&callable).unwrap()), BTreeSet::from([user]));
    }

    #[test]
    fn filters_below_confidence_threshold() {
        let callable = id(1);
        let user = id(2);
        let facts = ReturnFacts {
            returns: vec![ReturnFact {
                callable_id: callable,
                expr: ReturnExpr::Type(user),
                evidence: TypeEvidence { confidence: TypeConfidence::Medium, source: TypeEvidenceSource::ReturnAnnotation },
            }],
            ..Default::default()
        };

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());

        assert!(result.summaries.get(&callable).map_or(true, |s| s.returns().next().is_none()));
    }

    #[test]
    fn propagates_assignment_types_from_call_returns() {
        let caller = id(1);
        let factory = id(2);
        let service = id(3);
        let facts = ReturnFacts {
            returns: vec![ReturnFact {
                callable_id: factory,
                expr: ReturnExpr::Type(service),
                evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
            }],
            assignments: vec![AssignmentFact {
                scope_id: caller,
                variable: "svc".to_string(),
                expr: ReturnExpr::Call(factory),
                evidence: TypeEvidence::high(TypeEvidenceSource::ReturnCall),
            }],
            ..Default::default()
        };

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());

        let svc_summary = result.variable_summaries.get(&(caller, "svc".to_string())).unwrap();
        assert_eq!(returns(svc_summary), BTreeSet::from([service]));
    }

    #[test]
    fn resolves_receiver_calls_from_multi_return_summaries() {
        let caller = id(1);
        let factory = id(2);
        let admin = id(3);
        let guest = id(4);
        let admin_display = id(5);
        let guest_display = id(6);
        let facts = ReturnFacts {
            returns: vec![
                ReturnFact {
                    callable_id: factory,
                    expr: ReturnExpr::Type(admin),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
                },
                ReturnFact {
                    callable_id: factory,
                    expr: ReturnExpr::Type(guest),
                    evidence: TypeEvidence::high(TypeEvidenceSource::ReturnConstructor),
                },
            ],
            assignments: vec![AssignmentFact {
                scope_id: caller,
                variable: "user".to_string(),
                expr: ReturnExpr::Call(factory),
                evidence: TypeEvidence::high(TypeEvidenceSource::ReturnCall),
            }],
            ..Default::default()
        };
        let resolver = FakeMemberResolver::default()
            .with_method(admin, "display_info", admin_display)
            .with_method(guest, "display_info", guest_display);

        let result = ReturnSolver::solve(&facts, ReturnSolverOptions::default());
        let candidates = result.resolve_receiver_calls(
            &[ReceiverCallFact {
                caller_id: caller,
                receiver: ReturnExpr::Variable("user".to_string()),
                method_name: "display_info".to_string(),
                position: crate::core::Position::new(0, 0, 0),
            }],
            &resolver,
            TypeConfidence::High,
        );
        let callees = candidates.iter().map(|c| c.callee_id).collect::<BTreeSet<_>>();

        assert_eq!(callees, BTreeSet::from([admin_display, guest_display]));
        assert!(candidates.iter().all(|c| c.polymorphic));
    }
}
