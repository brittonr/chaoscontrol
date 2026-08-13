let OperatorCommand = import "../../operator-command.ncl" in
"replay-readiness --receipt target/replay-readiness.json" | OperatorCommand
