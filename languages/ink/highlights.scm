; Narrative (text) nodes intentionally have no capture.
(comment) @comment
(todo) @comment.doc
(identifier) @variable
(number) @number
(boolean) @boolean
(string) @string
(escape) @string.escape
(tag) @tag
(include_path) @string.special

(knot name: (identifier) @title)
(stitch name: (identifier) @title)
(function name: (identifier) @function)
(external_declaration name: (identifier) @function)
(call name: (identifier) @function)
(parameter name: (identifier) @variable.parameter)
(constant_declaration name: (identifier) @constant)
(list_item name: (identifier) @constant)
(label name: (identifier) @label)
(path (identifier) @label)
((path (identifier) @constant.builtin)
  (#any-of? @constant.builtin "END" "DONE"))

["VAR" "CONST" "LIST" "EXTERNAL" "function" "temp" "return" "ref" "else"] @keyword
"INCLUDE" @preproc
(sequence_type) @keyword
["not" "and" "or" "has" "hasnt" "mod"] @operator
["+" "-" "*" "/" "%" "^" "!" "?" "!?" "=" "+=" "-=" "++" "--"
 "==" "!=" "<" ">" "<=" ">=" "&&" "||"] @operator
["->" "<-" "->->" "~"] @operator
(glue) @operator
(sequence_modifier) @punctuation.special
(knot_marker) @punctuation.special
(choice_marker) @punctuation.list_marker
(gather_marker) @punctuation.list_marker
(branch "-" @punctuation.list_marker)
(sequence_branch "-" @punctuation.list_marker)
["{" "}" "[" "]" "(" ")"] @punctuation.bracket
[":" "," "." "|"] @punctuation.delimiter
