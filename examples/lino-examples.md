# LINO REST API Examples

This document provides examples of using the LINO REST API.

## API Endpoints

### GET /api/links
Get all links from the database.

**Example Request:**
```bash
curl -X GET http://localhost:5000/api/links
```

**Example Response:**
```json
{
  "query": "((($i: $s $t)) (($i: $s $t)))",
  "linksBefore": "",
  "linksAfter": "(1: 1 1)\n(2: 2 2)",
  "changes": [],
  "changeCount": 0
}
```

### POST /api/links
Create new links using LINO syntax.

**Example Request - Create single link:**
```bash
curl -X POST http://localhost:5000/api/links \
  -H "Content-Type: application/json" \
  -d '{"query": "() ((1 1))"}'
```

**Example Request - Create multiple links:**
```bash
curl -X POST http://localhost:5000/api/links \
  -H "Content-Type: application/json" \
  -d '{"query": "() ((1 1) (2 2))"}'
```

### PUT /api/links
Update existing links using LINO syntax.

**Example Request - Update link target:**
```bash
curl -X PUT http://localhost:5000/api/links \
  -H "Content-Type: application/json" \
  -d '{"query": "((1: 1 1)) ((1: 1 2))"}'
```

### DELETE /api/links
Delete links using LINO syntax.

**Example Request - Delete specific link:**
```bash
curl -X DELETE http://localhost:5000/api/links \
  -H "Content-Type: application/json" \
  -d '{"query": "((1 2)) ()"}'
```

**Example Request - Delete all links:**
```bash
curl -X DELETE http://localhost:5000/api/links \
  -H "Content-Type: application/json" \
  -d '{"query": "((* *)) ()"}'
```

### POST /api/links/query
Execute arbitrary LINO query.

**Example Request - Read with variables:**
```bash
curl -X POST http://localhost:5000/api/links/query \
  -H "Content-Type: application/json" \
  -d '{"query": "((($index: $source $target)) (($index: $target $source)))"}'
```

**Example Request - With trace enabled:**
```bash
curl -X POST http://localhost:5000/api/links/query \
  -H "Content-Type: application/json" \
  -d '{"query": "((($i: $s $t)) (($i: $s $t)))", "trace": true}'
```

## LINO Syntax Reference

### Basic Link Format
- `(source target)` - Link with source and target, auto-assigned index
- `(index: source target)` - Link with specific index, source, and target

### Variables
- `$i` or `$index` - Variable for index
- `$s` or `$source` - Variable for source  
- `$t` or `$target` - Variable for target

### Wildcards
- `*` - Matches any value
- `((* *))` - Matches all links

### Query Structure
LINO queries have two parts:
1. **Matching pattern** - What to find
2. **Substitution pattern** - What to replace it with

Format: `((matching pattern)) ((substitution pattern))`

### Examples
- `() ((1 1))` - Create link with source=1, target=1
- `((1: 1 1)) ((1: 1 2))` - Update link 1 to have target=2
- `((1 2)) ()` - Delete link with source=1, target=2
- `((($i: $s $t)) (($i: $s $t)))` - Read all links (no change)

## Response Format

All endpoints return a JSON response with:
- `query` - The LINO query that was executed
- `linksBefore` - State of database before query (if applicable)
- `linksAfter` - State of database after query
- `changes` - List of changes made
- `changeCount` - Number of changes made