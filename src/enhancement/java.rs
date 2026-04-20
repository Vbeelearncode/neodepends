//! Java dependency enhancement.
//!
//! Depends resolves structural Java deps (Import, Extend, Call to named methods) but misses
//! four intra-class delegation patterns that only appear at the statement level:
//!
//! 1. **Constructor field assignments** — `this.name = name` is a Use dep from the constructor
//!    to the field, but Depends emits nothing for it.
//!
//! 2. **Super-constructor delegation** — `super(x, y)` is a Call from the child constructor to
//!    the parent constructor. Depends does not emit this edge.
//!
//! 3. **This-constructor delegation** — `this(x)` is a Call from one constructor overload to a
//!    sibling constructor. Depends does not emit this edge either.
//!
//! 4. **Method overrides** — a method annotated `@Override` has a structural relationship to
//!    the inherited method it replaces, but Depends emits no Override-typed edge for it.

use std::collections::HashMap;

use crate::core::DepKind;
use crate::core::EntityKind;
use crate::enhancement::entity_index::{dep_at_row, dedup_edges, EntityIndex};
use crate::enhancement::DepEnhancer;

#[derive(Debug)]
pub struct JavaConstructorHeuristic;

impl DepEnhancer for JavaConstructorHeuristic {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[crate::core::Entity],
        deps: Vec<crate::core::EntityDep>,
    ) -> Vec<crate::core::EntityDep> {
        let index = EntityIndex::build(sources, entities, &deps);
        let mut extra_deps = Vec::new();

        for (filename, content) in sources {
            if !filename.ends_with(".java") {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let Some(file_cid) = index.content_id_of_file(filename) else { continue };

            for entity in entities {
                if entity.content_id != file_cid || entity.kind != EntityKind::Constructor {
                    continue;
                }
                let Some(class_id) = entity.parent_id else { continue };
                let body_start = entity.code.start.row.min(lines.len());
                let body_end = entity.code.end.row.min(lines.len());
                let body_lines = &lines[body_start..body_end];
                let commit_id = index.commit_id_of_entity(entity.id);

                for (offset, line) in body_lines.iter().enumerate() {
                    let row = entity.code.start.row + offset;
                    for field_name in this_field_assignments_in_line(line) {
                        if let Some(field_id) = index.resolve_field(class_id, &field_name) {
                            extra_deps.push(dep_at_row(entity.id, field_id, DepKind::Use, row, commit_id));
                        }
                    }
                }

                if body_lines.iter().any(|l| line_has_super_call(l)) {
                    if let Some(bases) = index.bases_of(class_id) {
                        for &base_id in bases {
                            let base_name = index.entity(base_id).map(|e| e.name.as_str()).unwrap_or("");
                            if let Some(base_ctor_id) = index.find_inherited_member(base_id, EntityKind::Constructor, base_name) {
                                extra_deps.push(dep_at_row(entity.id, base_ctor_id, DepKind::Call, entity.code.start.row, commit_id));
                            } else {
                                extra_deps.push(dep_at_row(entity.id, base_id, DepKind::Call, entity.code.start.row, commit_id));
                            }
                        }
                    }
                }

                if body_lines.iter().any(|l| line_has_this_call(l)) {
                    for &sibling_id in index.children_of(class_id) {
                        if sibling_id == entity.id {
                            continue;
                        }
                        if index.entity(sibling_id).map_or(false, |e| e.kind == EntityKind::Constructor) {
                            extra_deps.push(dep_at_row(entity.id, sibling_id, DepKind::Call, entity.code.start.row, commit_id));
                        }
                    }
                }
            }
        }

        let mut all_deps = deps;
        all_deps.extend(extra_deps);
        dedup_edges(&mut all_deps);
        all_deps
    }
}

#[derive(Debug)]
pub struct JavaOverrideHeuristic;

impl DepEnhancer for JavaOverrideHeuristic {
    fn enhance(
        &self,
        sources: &HashMap<String, String>,
        entities: &[crate::core::Entity],
        deps: Vec<crate::core::EntityDep>,
    ) -> Vec<crate::core::EntityDep> {
        let index = EntityIndex::build(sources, entities, &deps);
        let mut extra_deps = Vec::new();

        for (filename, content) in sources {
            if !filename.ends_with(".java") {
                continue;
            }
            let lines: Vec<&str> = content.lines().collect();
            let Some(file_cid) = index.content_id_of_file(filename) else { continue };

            for entity in entities {
                if entity.content_id != file_cid || entity.kind != EntityKind::Method {
                    continue;
                }
                let Some(class_id) = entity.parent_id.filter(|&pid| {
                    index.entity(pid).map_or(false, |p| p.kind == EntityKind::Class)
                }) else {
                    continue
                };

                if !has_override_annotation_before_row(entity.code.start.row, &lines) {
                    continue;
                }

                let commit_id = index.commit_id_of_entity(entity.id);
                if let Some(bases) = index.bases_of(class_id) {
                    for &base_id in bases {
                        if let Some(parent_method_id) =
                            index.find_inherited_member(base_id, EntityKind::Method, &entity.name)
                        {
                            extra_deps.push(dep_at_row(
                                entity.id,
                                parent_method_id,
                                DepKind::Override,
                                entity.code.start.row,
                                commit_id,
                            ));
                            break;
                        }
                    }
                }
            }
        }

        let mut all_deps = deps;
        all_deps.extend(extra_deps);
        dedup_edges(&mut all_deps);
        all_deps
    }
}

fn this_field_assignments_in_line(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = line.trim();
    while let Some(pos) = rest.find("this.") {
        rest = &rest[pos + 5..];
        let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '_').unwrap_or(rest.len());
        let name = &rest[..name_end];
        if !name.is_empty() {
            let after_name = rest[name_end..].trim_start();
            if after_name.starts_with('=') && !after_name.starts_with("==") {
                names.push(name.to_string());
            }
        }
        rest = &rest[name_end..];
    }
    names
}

fn line_has_super_call(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("super(") || t.contains(" super(") || t.contains("\tsuper(")
}

fn line_has_this_call(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("this(") || t.contains(" this(") || t.contains("\tthis(")
}

fn has_override_annotation_before_row(method_row: usize, lines: &[&str]) -> bool {
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

    #[test]
    fn this_field_assignments_finds_basic_assignment() {
        let names = this_field_assignments_in_line("        this.name = value;");
        assert_eq!(names, vec!["name"]);
    }

    #[test]
    fn this_field_assignments_skips_equality_comparison() {
        let names = this_field_assignments_in_line("        if (this.name == other) {");
        assert!(names.is_empty());
    }

    #[test]
    fn this_field_assignments_finds_multiple_on_same_line() {
        let names = this_field_assignments_in_line("this.x = 1; this.y = 2;");
        assert_eq!(names, vec!["x", "y"]);
    }

    #[test]
    fn has_override_annotation_detects_annotation() {
        let lines = vec!["    @Override", "    public void foo() {"];
        assert!(has_override_annotation_before_row(1, &lines));
    }

    #[test]
    fn has_override_annotation_returns_false_when_absent() {
        let lines = vec!["    public void foo() {"];
        assert!(!has_override_annotation_before_row(0, &lines));
    }

    #[test]
    fn has_override_annotation_ignores_comment_lines_between() {
        let lines = vec!["    @Override", "    // some comment", "    public void foo() {"];
        assert!(has_override_annotation_before_row(2, &lines));
    }

    #[test]
    fn line_has_super_call_detects_leading_super() {
        assert!(line_has_super_call("        super(x, y);"));
    }

    #[test]
    fn line_has_super_call_false_for_superclass_ref() {
        assert!(!line_has_super_call("        SuperClass obj = new SuperClass();"));
    }

    #[test]
    fn line_has_this_call_detects_delegating_constructor() {
        assert!(line_has_this_call("        this(defaultValue);"));
    }
}
