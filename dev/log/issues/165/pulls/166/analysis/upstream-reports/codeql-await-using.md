CodeQL 2.27.4 reports a syntax error for a valid call to a function named `using` when it is awaited. Found by checking extraction diagnostics in https://github.com/link-foundation/link-cli/pull/166.

Minimal ECMAScript module:

```js
async function using(store, action) { return action(store); }
await using({}, async () => {});
```

`node --input-type=module < repro.mjs` succeeds (Node 24 locally; Node 26, Bun and Deno accepted the original downstream conformance suite). CodeQL JavaScript extraction reports `Syntax error: unexpected token '('` at the call, and SARIF includes `js/syntax-error` despite an empty results array and successful invocation. Real failing run: https://github.com/link-foundation/link-cli/actions/runs/42130454613 (CodeQL javascript-typescript).

The apparent parser cause is `javascript/extractor/src/com/semmle/js/parser/Parser.java`: `isAwaitUsing()` checks only the words `await using`, and `parseStatement()` then selects `parseVarStatement(..., "using")` without requiring a following binding identifier. This misclassifies the call as an await-using declaration. `isUsingDecl()` already has binding lookahead for unawaited `using`.

Expected: parse the expression statement as an awaited function call. Suggested fix: require valid declaration binding lookahead after `await using` before choosing the declaration production; retain actual `await using resource = expression` parsing. Add both forms as parser regression fixtures.

Workaround: rename the helper (downstream uses `withClient`) while keeping all conformance scenarios. Reproducer and collected SARIF: https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166 . The standalone reproducer is `experiments/issue-165/codeql-using-repro.mjs.txt` (text suffix prevents the known parser defect from breaking its own audit).
