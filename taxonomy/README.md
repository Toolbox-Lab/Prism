# Enhanced Error Taxonomy Database

## Schema v2.0

New fields added per [Issue #381](https://github.com/Toolbox-Lab/Grat/issues/381):

### Related Errors

- `[[error.ERROR_CODE.related_errors]]` - Link similar or related errors
- Each entry has: `code`, `relationship` (similar/precondition/cascade), `description`

### Documentation URLs

- `[error.ERROR_CODE.documentation]` - Links to official docs
- Fields: `stellar_docs_url`, `soroban_docs_url`

### Since Protocol Version

- `[error.ERROR_CODE.protocol]` - Protocol version tracking
- Fields: `since_version`, `deprecated_in`, `removed_in`

### Contract-Specific Examples

- `[[error.ERROR_CODE.examples]]` - Real Soroban contract examples
- Fields: `contract`, `scenario`, `code_snippet`

### Schema Metadata

- `[metadata]` - Schema version and field requirements
