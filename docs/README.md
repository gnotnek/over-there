# Over There documentation

| Document | Read it when you want to know… |
| --- | --- |
| [architecture.md](architecture.md) | How the system fits together (context, layers, folders, request lifecycle, where new code goes) and why the stack and boundaries were chosen |
| [data-catalog.md](data-catalog.md) | What is stored, where, what each column means, how it is derived, and who may see it |
| [service-catalog.md](service-catalog.md) | Which module or external service does what, its interface, its dependencies and what happens when it fails |
| [contracts.md](contracts.md) | What each endpoint accepts and returns, cookies, CSRF, file formats, template-to-script data, and configuration |
| [learning-rust.md](learning-rust.md) | How the Rust in this codebase works, with exercises |
| [roadmap.md](roadmap.md) | What is done, what is next, and what is not planned |
| [specs/](specs) and [plans/](plans) | The design and implementation plan for each phase |

## Keeping these current

Update the reference docs in the same pull request as the change:

- New or changed migration, stored file, or derived column → [data-catalog.md](data-catalog.md)
- New module, script, background task or third-party service → [service-catalog.md](service-catalog.md)
- New or changed endpoint, form field, cookie, file format, `data-*` attribute or environment variable → [contracts.md](contracts.md)
- New layer, process or boundary → [architecture.md](architecture.md)

If a document and the code disagree, the code is right and the document is a bug.
