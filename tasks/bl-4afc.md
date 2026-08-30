+++
title = "promote-changelog is era-blind at the fence: its duplicate guard matches lernie-era headings and its compare URL writes bare v-tags"
created = 1788059938
updated = 1788068014
claimant = "OrderCutter"
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
Two residues of the bl-2f58 fence in the Makefile's promote-changelog target, neither biting until the 0.0.2 release. (1) The duplicate guard greps '^## [VERSION]' across the whole changelog, so every litany version up to 0.0.11 is refused by the lernie-era heading that already carries that number — the guard needs to distinguish eras (the litany-era headings link to the litany compare URLs; the lernie-era ones do not). (2) The compare-URL sed writes v<prev>...v<version>, but litany-era tags are litany-v<version> (release-plz.toml git_tag_name, bl-2f58) — the link it writes for 0.0.2 would name two tags that do not exist. The 0.0.1 section was stamped by hand with a cross-fence URL (v0.0.11...litany-v0.0.1); fix the target before the next promotion, and keep the prev-extraction consistent with whichever era spelling the guard adopts.