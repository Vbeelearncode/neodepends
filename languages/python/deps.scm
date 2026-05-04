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

(call
  function: (attribute
    object: (attribute
      object: (identifier) @_self_field_self
      attribute: (identifier) @call.self_field_recv)
    attribute: (identifier) @call.self_field_method)
  (#eq? @_self_field_self "self"))

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

(function_definition
  parameters: (parameters
    (identifier) @function.param))

(function_definition
  parameters: (parameters
    (typed_parameter
      (identifier) @function.param)))

(expression_statement
  (assignment
    left: (attribute
      object: (identifier) @_field_assign_self
      attribute: (identifier) @field_assign.field)
    right: (identifier) @field_assign.param)
  (#eq? @_field_assign_self "self"))

(expression_statement
  (assignment
    left: (identifier) @assign.var
    right: (call
      function: (identifier) @assign.class
      arguments: (argument_list))))

(expression_statement
  (assignment
    left: (identifier) @return_assign.var
    right: (call
      function: (identifier) @return_assign.call
      arguments: (argument_list))))

(expression_statement
  (assignment
    left: (identifier) @return_assign_attr.var
    right: (call
      function: (attribute
        object: (identifier) @return_assign_attr.recv
        attribute: (identifier) @return_assign_attr.method)
      arguments: (argument_list))))

(return_statement
  (identifier) @return.var)

(return_statement
  (attribute
    object: (identifier) @_return_self
    attribute: (identifier) @return.self_field)
  (#eq? @_return_self "self"))

(return_statement
  (call
    function: (identifier) @return.call
    arguments: (argument_list)))

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
