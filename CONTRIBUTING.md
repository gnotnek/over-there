# Contributing to Over There

Thanks for helping riders find and share good routes. Open an issue before a large change so the product scope and domain rules are clear. Small bug fixes can go straight to a pull request.

1. Read `docs/architecture.md` for domain boundaries and privacy rules.
2. Run `cargo fmt --check`, `cargo test`, and `cargo check`.
3. Include a domain test for changes to GPX metrics, access policy or visibility.
4. Keep uploaded GPX and user data out of commits. Use synthetic fixtures in tests.
5. Describe user impact, migration needs and the checks you ran in the pull request.

The project follows the [Contributor Covenant 2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/). Report security issues privately through GitHub's security advisory workflow when the public repository is available.
