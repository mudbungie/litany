+++
title = "Ingest's refused-push red line names whose case it is: main did not move, so it is a hand's"
created = 1795072278
updated = 1795072279
claimant = "Knuckled-L"
priority = 1
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Wording and spelling only, after bl-b9fb: when the push to main is refused and the fetched main is still an ancestor of the branch, the run prints 'the push to main was refused and main did not move: this is a hand's' and fails; the ancestor test names the branch rather than HEAD. The forgiven arm is unchanged.