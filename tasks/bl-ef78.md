+++
title = "run notify_ui / mark_abandoned at a child's verdict event: the watcher's natural landing (DESIGN_PROCEDURES §7, left open by bl-f490)"
created = 1792633753
updated = 1792812440
claimant = "Sheepdog"
priority = 3
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
execute_child (src/prompt/dispatch/child_result.rs) declines MarkAbandoned/NotifyUi with ActionUnsupported; cases.rs an_unsupported_child_result_action_is_declined_loudly and tests/gate.rs a_verdict_action_without_an_executor_is_declined pin that. A verdict about the dispatcher's situation (guard_injected) is what a mark is for: run workflow_actions::execute for the two marks there, flip the two pins, and reword DESIGN_PROCEDURES §7 back to notify_ui as the watcher's landing. bl-f490 landed only the doc sentence stating the gap.