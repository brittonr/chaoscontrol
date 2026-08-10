let OperatorCommand = import "../../operator-command.ncl" in
let base = import "../../examples/typed-operator-command.ncl" in
(base & {working_directory = "../escape"}) | OperatorCommand
