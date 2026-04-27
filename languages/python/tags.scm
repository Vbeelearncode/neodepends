; Python entity tagging queries for NeoDepends
;
; NeoDepends uses tree-sitter queries to identify entities (File/Class/Method/Field)
; and their spans. Dependencies are later mapped onto these entities by position.
;
; Notes:
; - These queries do NOT need to “match” the stack-graphs.tsg; they operate on the same
;   tree-sitter parse tree but serve a different purpose (entity extraction vs name resolution).
; - Field tagging for Python is inherently approximate: assignments occur inside methods, so
;   NeoDepends will often parent Field entities under the enclosing Method. Our Python
;   enhancement step can re-parent those Fields to the Class and dedupe them.

; ---------------------------------------------------------------------------
; Classes
; ---------------------------------------------------------------------------
(
  (class_definition
    name: (identifier) @name) @tag.Class
)

; NOTE: We intentionally avoid a secondary "compatibility" pattern here.
; In practice, matching both `name: (identifier)` and plain `(identifier)` will double-tag
; the same class on modern tree-sitter-python, creating duplicate Class entities.

; NOTE: We intentionally do not tag `decorated_definition` wrappers as Class.
; The inner `class_definition` is always present and already matched above.
; Tagging both would duplicate Class entities for decorated classes (e.g. @dataclass),
; which breaks downstream indexing and creates Missing/Extra edges.

; ---------------------------------------------------------------------------
; Functions and Methods
; ---------------------------------------------------------------------------
; IMPORTANT: Distinguish between module-level functions and class methods.
;
; - Module-level functions (top-level in a .py file) are tagged as Function
; - Class methods (defined inside a class body) are tagged as Method
;
; If we tag every `function_definition` node unconditionally, we also tag nested
; helper functions inside methods (e.g., `def filter(...):` inside `apply()`).
; That inflates the entity set and moves dependencies to the wrong owner
; (extra/missing edges vs ground-truth).
;
; Module-level function: `def f(...): ...`
(
  (module
    (function_definition
      name: (identifier) @name) @tag.Function)
)
; Decorated module-level function: `@decorator\ndef f(...): ...`
(
  (module
    (decorated_definition
      (function_definition
        name: (identifier) @name) @tag.Function))
)
; __init__ constructor
(
  (class_definition
    body: (block
      (function_definition
        name: (identifier) @name (#eq? @name "__init__")) @tag.Constructor))
)
; Direct class method (excluding __init__)
(
  (class_definition
    body: (block
      (function_definition
        name: (identifier) @name (#not-eq? @name "__init__")) @tag.Method))
)
; Decorated class method: `class C: @decorator\ndef m(...): ...`
(
  (class_definition
    body: (block
      (decorated_definition
        (function_definition
          name: (identifier) @name) @tag.Method)))
)

; ---------------------------------------------------------------------------
; Fields (instance attributes / class attributes)
; ---------------------------------------------------------------------------
; Instance field assignment: self.field = ...
(
  (expression_statement
    (assignment
      left: (attribute
        object: (identifier) @self_ref
        attribute: (identifier) @name)
    )
  ) @tag.Field
  (#eq? @self_ref "self")
)

; Instance field augmented assignment: self.field += ...
(
  (expression_statement
    (augmented_assignment
      left: (attribute
        object: (identifier) @self_ref
        attribute: (identifier) @name)
    )
  ) @tag.Field
  (#eq? @self_ref "self")
)

; Optional: treat cls.field = ... inside classmethods as a class-field
(
  (expression_statement
    (assignment
      left: (attribute
        object: (identifier) @cls_ref
        attribute: (identifier) @name)
    )
  ) @tag.Field
  (#eq? @cls_ref "cls")
)

; Class-body field assignment: field = ...
; This captures simple class attributes declared directly in the class suite.
(
  (class_definition
    body: (block
      (expression_statement
        (assignment
          left: (identifier) @name
        ) @tag.Field
      )
    )
  )
)

; NOTE: Some tree-sitter-python versions do not expose a dedicated
; `annotated_assignment` node type. To keep this query compatible across
; versions, we intentionally do not match annotated assignments here.
