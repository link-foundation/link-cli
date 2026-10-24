---
bump: patch
---

Fix Auto Release compilation with warnings denied by importing the test-only
PathBuf in the test module. Compile every executable release helper in its
non-test release configuration on pull requests before it can reach main.
