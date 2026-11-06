# Full template tree comparison

Every blob in all four recursive trees is inventoried in `template-file-matrix.csv`. CI/workflow/helper/config/test snapshots are stored beside each tree at the immutable SHA below. Product source, generated dependencies and template demonstration applications are classified separately; they are not runnable CI in this repository.

| Template | Commit | Tree blobs | Content snapshots |
|---|---|---:|---:|
| rust | `bd98923e89f90a92d49ed31bd118062321d05353` | 228 | 110 |
| csharp | `88bb71832e04a1a52ddb584f1db0f57b6e966646` | 148 | 99 |
| js | `7877a27980664bc6453e80262f8391d1c2562ab1` | 532 | 201 |
| python | `78f2fdf5e085871387555bc066071f0aedd24479` | 155 | 89 |

The matrix records per-file dispositions, including supporting tests and tool configs. `template-content-inventory.md` records workflow jobs, helper functions and commands inspected in the snapshots. The audit report maps the shared behaviors to fixes and verification. No template-specific desktop, Docker or Python distribution is introduced into link-cli.
