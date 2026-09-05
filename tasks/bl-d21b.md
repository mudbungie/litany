+++
title = "brazen 0.0.13 reverses the bl-4c64 survey: a row may DECLARE a context window its list does not serve, and that window rides the Usage event — so the shipped-default refusal and two doc sentences are now stale"
created = 1788581602
updated = 1788581602
priority = 2
root_commit = "12899370c9ec7a5ed7f8e26d3d4fb914ea6c3310"
+++
brazen kept releasing while the pin chain ran. At the moment bl-a825 edited Cargo.toml the newest published brazen was 0.0.12; 0.0.13 and 0.0.14 followed within the hour, and 0.0.13 is not infrastructure.

0.0.13 is brazen bl-c655: a provider row may state the context window its list does not serve, and that window rides the Usage event like a served one. That is exactly the change bl-4c64's survey named as the one thing that would reverse its verdict. Its ARCH shipped-state note reads, verbatim: 'What would reverse this is a change in brazen, not in litany: built-in rows naming a context_key their provider actually serves — Anthropic and OpenAI both publish per-model context limits, they are simply not lifted today — or a window that does not depend on a discovery call having been run.' A declared window is the second of those two.

Two more sentences go stale with it, both written by bl-3fe6 and both saying the opposite of what now holds. docs/DESIGN_CONTEXT_ECONOMY.md section 5.1's 'A window an operator sets is still not a window brazen states' argues that options.num_ctx changes the window in force without putting a context_window on the Usage event; a declared window does put one there, so the paragraph's conclusion — 'until brazen states it, that operator's row is still declined' — is now false for a row whose operator declares one. And the same section's 'what would move it is unchanged: a change in brazen's rows' has been overtaken.

So: bump the pin to the newest published brazen (0.0.14 or later — crates.io at the moment of the edit is the check, Cargo.toml the one home, README gate-checked against it), then re-read the survey against the declared-window mechanism and amend both docs to what holds. The open question the re-read must answer is whether the shipped default may now assume a window: a DECLARED window is authored configuration, not a served fact, so it is present only where an operator wrote one — which may leave the refusal standing for a different reason than the one recorded. Decide it, do not assume it.

Gates: tests 100 percent, docs, alignment.