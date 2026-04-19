; Classes

(
  (comment)? @comment
  .
  (class_declaration
    name: (type_identifier) @name) @tag.Class
)

(
  (comment)? @comment
  .
  (abstract_class_declaration
    name: (type_identifier) @name) @tag.Class
)

; Interfaces

(
  (comment)? @comment
  .
  (interface_declaration
    name: (type_identifier) @name) @tag.Interface
)

; Enums

(
  (comment)? @comment
  .
  (enum_declaration
    name: (identifier) @name) @tag.Enum
)

; Module-level functions (restricted to `program` scope so nested helpers
; inside methods don't get tagged)

(program
  (function_declaration
    name: (identifier) @name) @tag.Function)

(program
  (export_statement
    (function_declaration
      name: (identifier) @name) @tag.Function))

(program
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: (arrow_function))) @tag.Function)

(program
  (export_statement
    (lexical_declaration
      (variable_declarator
        name: (identifier) @name
        value: (arrow_function)))) @tag.Function)

; Constructors (disjoint from Method via #eq?/#not-eq?)

(
  (comment)? @comment
  .
  (method_definition
    name: (property_identifier) @name) @tag.Constructor
  (#eq? @name "constructor")
)

; Methods

(
  (comment)? @comment
  .
  (method_definition
    name: (property_identifier) @name) @tag.Method
  (#not-eq? @name "constructor")
)

(
  (comment)? @comment
  .
  (method_signature
    name: (property_identifier) @name) @tag.Method
)

(
  (comment)? @comment
  .
  (abstract_method_signature
    name: (property_identifier) @name) @tag.Method
)

; Fields

(
  (comment)? @comment
  .
  (public_field_definition
    name: (property_identifier) @name) @tag.Field
)

(
  (comment)? @comment
  .
  (property_signature
    name: (property_identifier) @name) @tag.Field
)
