(attribute
  object: (identifier) @_self
  attribute: (identifier) @use.self_field
  (#eq? @_self "self"))

(call
  function: (attribute
    object: (identifier) @_self
    attribute: (identifier) @call.self_method)
  (#match? @_self "^(self|cls)$"))

(call
  function: (attribute
    object: (call
      function: (identifier) @_super)
    attribute: (identifier) @call.super_method)
  (#eq? @_super "super"))

(call
  function: (attribute
    object: (identifier) @call.class_recv
    attribute: (identifier) @call.class_method))

(class_definition
  (argument_list
    (identifier) @extend.base))

(import_statement
  name: (dotted_name) @import.module)

(import_from_statement
  module_name: (dotted_name) @import_from.module)

(call
  function: (identifier) @create.class
  arguments: (argument_list))

(parameters
  (typed_parameter
    (identifier) @param.name
    type: (type (identifier) @param.type)))

(expression_statement
  (assignment
    left: (identifier) @assign.var
    right: (call
      function: (identifier) @assign.class
      arguments: (argument_list))))

(decorated_definition
  (decorator
    (identifier) @_deco)
  definition: (function_definition
    name: (identifier) @abstract.method)
  (#eq? @_deco "abstractmethod"))

(function_definition
  name: (identifier) @abstract.method
  body: (block
    (raise_statement
      (call
        function: (identifier) @_not_impl)))
  (#eq? @_not_impl "NotImplementedError"))
