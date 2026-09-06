+++
title = "read_file has no offset or limit, so the 4 KiB tool_output bound cuts a whole-file read in the middle and the model re-reads with sed"
created = 1788675642
updated = 1788675642
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
tags = ["usability-r2"]
+++
bl-ce09 (3c03e468) set tool_output to 2048/2048 bytes per stream; DESIGN_CONTEXT_ECONOMY §7.1 records that a whole-file read_file is cut mid-file and the remedy is a sed -n re-read. DESIGN_CODE_EXECUTION's standard corpus decisions name an offset/limit read; ship it: read_file takes optional offset and limit (lines), its result names the range it returned and the file's total, and a cut result says how to continue. Every comparator's read tool has this.