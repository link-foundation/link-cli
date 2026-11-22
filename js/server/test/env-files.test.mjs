// Unit tests of env-files.js, the port of `lino_arguments::init()` that
// rust/src/cli.rs runs before parsing the command line: `.lenv` through
// lino-env 0.1.0's `LinoEnv::read`, then `.env` through dotenvy 0.15.7's
// `from_path_iter`. The expected values of the dotenvy cases are those of
// the tests in dotenvy's src/parse.rs; the others were checked against the
// Rust crates.
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';
import { after, describe, it } from 'node:test';
import {
  loadEnvFile,
  loadEnvironmentFiles,
  loadLenvFile,
  parseDotenv,
  parseLinoEnv,
} from '../src/env-files.js';
import { main } from '../src/main.js';

const root = mkdtempSync(join(tmpdir(), 'clink-env-files-'));
after(() => rmSync(root, { recursive: true, force: true }));

let directories = 0;
/** A new directory holding `files` (name to string or bytes). */
function directory(files = {}) {
  directories += 1;
  const path = join(root, String(directories));
  mkdirSync(path);
  for (const [name, content] of Object.entries(files)) writeFileSync(join(path, name), content);
  return path;
}

/** The `{ key, value }` pairs and errors that dotenvy's `Iter` yields over `text`. */
const items = (text, env = {}) => [...parseDotenv(text, env)];
/** The pairs of `items`, failing on an error. */
const pairs = (text, env = {}) =>
  items(text, env).map((item) => {
    assert.equal(item.error, undefined, JSON.stringify(item));
    return [item.key, item.value];
  });

describe('parseLinoEnv (LinoEnv::read)', () => {
  it('splits at the first ": ", trims the key and keeps the value as is', () => {
    const text = 'GITHUB_TOKEN: gh_test123\n  SPACED  :  padded  \nURL: https://example.com:8080\n';
    assert.deepEqual(
      [...parseLinoEnv(text)],
      [
        ['GITHUB_TOKEN', 'gh_test123'],
        ['SPACED', ' padded  '],
        ['URL', 'https://example.com:8080'],
      ],
    );
  });

  it('keeps the last value of a repeated key in the place of the first', () => {
    const text = 'API_KEY: value1\nOTHER: x\nAPI_KEY: value2\nAPI_KEY: value3';
    assert.deepEqual(
      [...parseLinoEnv(text)],
      [
        ['API_KEY', 'value3'],
        ['OTHER', 'x'],
      ],
    );
  });

  it('skips blank lines, comments and lines without ": "', () => {
    const text = '\n   \n# A: 1\n  \t# B: 2\nNOSEP\nC:nospace\nD:\nEMPTY: \n';
    assert.deepEqual([...parseLinoEnv(text)], [['EMPTY', '']]);
  });

  it('drops "\\n" and "\\r\\n" but keeps the "\\r" of a last line without "\\n"', () => {
    assert.deepEqual(
      [...parseLinoEnv('A: 1\r\nB: 2\r')],
      [
        ['A', '1'],
        ['B', '2\r'],
      ],
    );
  });

  it('trims Unicode White_Space as Rust does, not a byte order mark', () => {
    assert.deepEqual(
      [...parseLinoEnv('\u3000Ü\u00a0: \u3000v\u3000\n\uFEFFK: v\n\u0085X: y\n\u0085#: no\n')],
      [
        ['Ü', '\u3000v\u3000'],
        ['\uFEFFK', 'v'],
        ['X', 'y'],
      ],
    );
  });

  it('reads an empty text as no variables', () => {
    assert.deepEqual([...parseLinoEnv('')], []);
  });
});

describe('parseDotenv (dotenvy Iter)', () => {
  it('parses the lines of test_parse_line_env', () => {
    // Note 5 spaces after 'KEY8='.
    const text = [
      '',
      'KEY=1',
      'KEY2="2"',
      "KEY3='3'",
      "KEY4='fo ur'",
      'KEY5="fi ve"',
      'KEY6=s\\ ix',
      'KEY7=',
      'KEY8=     ',
      'KEY9=   # foo',
      'KEY10  ="whitespace before ="',
      'KEY11=    "whitespace after ="',
      'export="export as key"',
      'export   SHELL_LOVER=1',
      '',
    ].join('\n');
    assert.deepEqual(pairs(text), [
      ['KEY', '1'],
      ['KEY2', '2'],
      ['KEY3', '3'],
      ['KEY4', 'fo ur'],
      ['KEY5', 'fi ve'],
      ['KEY6', 's ix'],
      ['KEY7', ''],
      ['KEY8', ''],
      ['KEY9', ''],
      ['KEY10', 'whitespace before ='],
      ['KEY11', 'whitespace after ='],
      ['export', 'export as key'],
      ['SHELL_LOVER', '1'],
    ]);
  });

  it('skips the comments of test_parse_line_comment', () => {
    assert.deepEqual(items('\n# foo=bar\n#    '), []);
  });

  it('fails each line of test_parse_line_invalid and goes on', () => {
    // Note 4 spaces after 'invalid'.
    const result = items('\n  invalid    \nvery bacon = yes indeed\n=value\nGOOD=1');
    assert.deepEqual(
      result.map((item) => item.error ?? item.key),
      ['LineParse', 'LineParse', 'LineParse', 'GOOD'],
    );
    assert.deepEqual(result[0], { error: 'LineParse', line: '  invalid    ', index: 9 });
  });

  it('unescapes the values of test_parse_value_escapes', () => {
    const text = [
      '',
      'KEY=my\\ cool\\ value',
      'KEY2=\\$sweet',
      'KEY3="awesome stuff \\"mang\\""',
      "KEY4='sweet $\\fgs'\\''fds'",
      'KEY5="\'\\"yay\\\\"\\ "stuff"',
      'KEY6="lol" #well you see when I say lol wh',
      'KEY7="line 1\\nline 2"',
      '',
    ].join('\n');
    assert.deepEqual(pairs(text), [
      ['KEY', 'my cool value'],
      ['KEY2', '$sweet'],
      ['KEY3', 'awesome stuff "mang"'],
      ['KEY4', "sweet $\\fgs'fds"],
      ['KEY5', '\'"yay\\ stuff'],
      ['KEY6', 'lol'],
      ['KEY7', 'line 1\nline 2'],
    ]);
  });

  it('fails the values of test_parse_value_escapes_invalid', () => {
    const text = "\nKEY=my uncool value\nKEY2=\"why\nKEY3='please stop''\nKEY4=h\\8u\n";
    const result = items(text);
    assert.ok(result.length > 0);
    for (const item of result) assert.equal(item.error, 'LineParse');
    assert.deepEqual(result[0], { error: 'LineParse', line: 'my uncool value', index: 3 });
  });

  it('reports the positions of dotenvy error_tests', () => {
    assert.deepEqual(items('\n    KEY=VALUE\n    KEY1=>${KEY{<\n    '), [
      { key: 'KEY', value: 'VALUE' },
      { error: 'LineParse', line: '>${KEY{<', index: 7 },
    ]);
    assert.deepEqual(items('.Key=VALUE'), [{ error: 'LineParse', line: '.Key=VALUE', index: 0 }]);
    assert.deepEqual(items('<><><>'), [{ error: 'LineParse', line: '<><><>', index: 0 }]);
    assert.deepEqual(items('VALUE=>\\f<'), [{ error: 'LineParse', line: '>\\f<', index: 2 }]);
  });

  it('counts parse_line positions in UTF-8 bytes and parse_value ones in characters', () => {
    assert.deepEqual(items('\u3000é=1'), [{ error: 'LineParse', line: '\u3000é=1', index: 3 }]);
    assert.deepEqual(items('K=é x'), [{ error: 'LineParse', line: 'é x', index: 2 }]);
    assert.deepEqual(items('K="é'), [{ error: 'LineParse', line: 'K="é', index: 5 }]);
  });

  it('fails `export` without a key or without "="', () => {
    assert.deepEqual(
      items('export\nexport 1A=1\nexport A\nexport =x\nexport A = 1').map(
        (item) => item.error ?? `${item.key}=${item.value}`,
      ),
      ['LineParse', 'LineParse', 'LineParse', 'export=x', 'A=1'],
    );
  });

  it('substitutes as the variable_substitution_tests expect', () => {
    assert.deepEqual(pairs('\n            KEY=test\n            KEY1="${KEY}"\n            '), [
      ['KEY', 'test'],
      ['KEY1', 'test'],
    ]);
    assert.deepEqual(pairs('KEY=">$KEY1<>${KEY2}<"'), [['KEY', '><><']]);
    assert.deepEqual(pairs('KEY=>\\$KEY1<>\\${KEY2}<'), [['KEY', '>$KEY1<>${KEY2}<']]);
    assert.deepEqual(pairs('KEY=">\\$KEY1<>\\${KEY2}<"'), [['KEY', '>$KEY1<>${KEY2}<']]);
    assert.deepEqual(pairs("KEY='>${KEY1}<>$KEY2<'"), [['KEY', '>${KEY1}<>$KEY2<']]);
    assert.deepEqual(pairs('\n    KEY=VALUE\n    KEY1=$KEY$KEY\n    '), [
      ['KEY', 'VALUE'],
      ['KEY1', 'VALUEVALUE'],
    ]);
    assert.deepEqual(pairs('\n    KEY.Value=VALUE\n    '), [['KEY.Value', 'VALUE']]);
    assert.deepEqual(
      pairs('\n            KEY=${KEY1}+KEY_VALUE\n            KEY1=${KEY}+KEY1_VALUE\n'),
      [
        ['KEY', '+KEY_VALUE'],
        ['KEY1', '+KEY_VALUE+KEY1_VALUE'],
      ],
    );
    assert.deepEqual(
      pairs('KEY1=test_user\nKEY1_1=test_user_with_separator\nKEY=">$KEY1_1<>$KEY1}<>$KEY1{<"'),
      [
        ['KEY1', 'test_user'],
        ['KEY1_1', 'test_user_with_separator'],
        ['KEY', '>test_user_1<>test_user}<>test_user{<'],
      ],
    );
    assert.deepEqual(
      pairs('\n    KEY1=test_user\n    KEY2=$KEY1_2\n    KEY=>${KEY1}<>${KEY2}<\n'),
      [
        ['KEY1', 'test_user'],
        ['KEY2', 'test_user_2'],
        ['KEY', '>test_user<>test_user_2<'],
      ],
    );
    assert.deepEqual(pairs('\n    KEY2=$KEY1_2\n    KEY=>${KEY1}<>${KEY2}<\n'), [
      ['KEY2', '_2'],
      ['KEY', '><>_2<'],
    ]);
  });

  it('prefers the environment to the earlier lines', () => {
    const env = { KEY11: 'test_user_env' };
    assert.deepEqual(pairs('KEY=">${KEY11}<"', env), [['KEY', '>test_user_env<']]);
    assert.deepEqual(pairs('\n    KEY11=test_user\n    KEY=">${KEY11}<"\n', env), [
      ['KEY11', 'test_user'],
      ['KEY', '>test_user_env<'],
    ]);
  });

  it('reads the environment lazily, as each line is parsed', () => {
    const env = {};
    const iterator = parseDotenv('A=1\nB=$A$A', env);
    assert.deepEqual(iterator.next().value, { key: 'A', value: '1' });
    env.A = 'set';
    assert.deepEqual(iterator.next().value, { key: 'B', value: 'setset' });
  });

  it('reproduces the corners of the substitution', () => {
    const env = { PRE: 'pre', 'A=B': 'no', '': 'no' };
    assert.deepEqual(
      items('E=\nV=x$E$\nW=${PRE}_${A=B}$\nX=$ y\nY=$A\'b\'\nZ="$PRE"\nQ="$PRE x"\nN=${é}$é', env),
      [
        { key: 'E', value: '' },
        { key: 'V', value: 'x' },
        { key: 'W', value: 'pre_' },
        { key: 'X', value: ' y' },
        // `'` ends `$A` and is kept, then opens a quote that never closes.
        { error: 'LineParse', line: "$A'b'", index: 4 },
        // `"` ends `$PRE` and is kept, so the quote stays open.
        { error: 'LineParse', line: '"$PRE"', index: 5 },
        { key: 'Q', value: 'pre x' },
        { key: 'N', value: '' },
      ],
    );
  });

  it('reads the process environment by default', () => {
    const name = `CLINK_ENV_FILES_DEFAULT_${process.pid}`;
    process.env[name] = 'from-process';
    try {
      assert.deepEqual([...parseDotenv(`A=\${${name}}`)], [{ key: 'A', value: 'from-process' }]);
    } finally {
      delete process.env[name];
    }
  });

  it('joins the lines of an open quote or a trailing escape', () => {
    const text = 'A="line1\nline2"\nB=\'x\n#not a comment\ny\'\nC=a\\\nD="\\"\n"\nE=\'a\\b\'\n';
    assert.deepEqual(pairs(text), [
      ['A', 'line1\nline2'],
      ['B', 'x\n#not a comment\ny'],
      ['C', 'a'],
      ['D', '"\n'],
      ['E', 'a\\b'],
    ]);
  });

  it('fails a quote, escape or odd trailing white space left open at the end', () => {
    assert.deepEqual(items('A=ok\nD="unterminated\nE=lost\n'), [
      { key: 'A', value: 'ok' },
      { error: 'LineParse', line: 'D="unterminated\nE=lost\n', index: 23 },
    ]);
    assert.deepEqual(items('A=a\\'), [{ error: 'LineParse', line: 'A=a\\', index: 4 }]);
    // `QuotedLines` takes `\'` in single quotes for an escape; `parse_value` does not.
    assert.deepEqual(items("E='\\'\n"), [{ error: 'LineParse', line: "E='\\'\n", index: 6 }]);
    assert.deepEqual(items('A=a '), [{ error: 'LineParse', line: 'A=a ', index: 4 }]);
    assert.deepEqual(items('A=a  '), [{ key: 'A', value: 'a' }]);
  });

  it('cuts comments after white space only and handles line endings', () => {
    const text = [
      '# c',
      '   # indented',
      'A=a #c',
      'B=a#b',
      'C="q" # c',
      'D=a \\ b',
      "E=a 'b'",
      'F=a "b"',
      "G='a'b",
      'H=a\tb',
      'I = i',
      '\r',
      'J=j\r',
      'K=k \r',
      // An even run of white space hides the `#` from `QuotedLines`.
      'M=a  #c',
      'L=l',
    ].join('\n');
    assert.deepEqual(
      items(text).map((item) => item.error ?? `${item.key}=${item.value}`),
      ['A=a', 'B=a#b', 'C=q', 'LineParse', 'LineParse', 'LineParse', 'G=ab', 'LineParse'].concat([
        'I=i',
        'J=j',
        'K=k',
        'M=a',
        'L=l',
      ]),
    );
    assert.deepEqual(items(''), []);
  });

  it('skips a byte line that is not UTF-8, with the lines joined before it', () => {
    const bytes = Buffer.concat([
      Buffer.from('A=1\nB="x\n'),
      Buffer.from([0xff, 0x0a]),
      Buffer.from('C=3\nD=é\nE=\uFEFF'),
      Buffer.from([0xc3]),
    ]);
    assert.deepEqual(items(bytes), [
      { key: 'A', value: '1' },
      { error: 'Io' },
      { key: 'C', value: '3' },
      { key: 'D', value: 'é' },
      { error: 'Io' },
    ]);
  });

  it('removes no byte order mark', () => {
    assert.deepEqual(
      items(Buffer.from('\uFEFFA=1\nB=2\n')).map((item) => item.error ?? item.key),
      ['LineParse', 'B'],
    );
  });
});

describe('loadEnvironmentFiles (lino_arguments::init)', () => {
  it('loads .lenv, then .env, without overwriting a variable already set', () => {
    const cwd = directory({
      '.lenv': 'A: lenv\nPRE: from-lenv\nF: a: b\n',
      '.env': 'A=env\nB=$A\nbad line\nC=${F}\nPRE=from-env\nD=1\nD=2\nE=$D\n',
    });
    const env = { PRE: 'pre' };
    loadEnvironmentFiles({ cwd, env });
    assert.deepEqual(env, {
      PRE: 'pre',
      A: 'lenv',
      F: 'a: b',
      B: 'lenv',
      C: 'a: b',
      D: '1',
      E: '1',
    });
  });

  it('ignores missing files, directories and a .lenv that is not UTF-8', () => {
    const env = {};
    loadEnvironmentFiles({ cwd: directory(), env });
    assert.deepEqual(env, {});
    const cwd = directory({ '.lenv': Buffer.from([0x41, 0x3a, 0x20, 0xff, 0x0a]) });
    mkdirSync(join(cwd, '.env'));
    loadEnvironmentFiles({ cwd, env });
    assert.deepEqual(env, {});
  });

  it('skips the variables Rust cannot set (it panics on them)', () => {
    const cwd = directory({
      '.lenv': ': empty\nA=B: eq\nN: a\0b\nOK: 1\n',
      '.env': 'V=a\0b\nW=2\n',
    });
    const env = {};
    assert.equal(loadLenvFile(join(cwd, '.lenv'), env), 1);
    assert.equal(loadEnvFile(join(cwd, '.env'), env), 1);
    assert.equal(loadLenvFile(join(cwd, 'missing'), env), 0);
    assert.equal(loadEnvFile(join(cwd, 'missing'), env), 0);
    assert.deepEqual(env, { OK: '1', W: '2' });
  });

  it('defaults to the current directory and the process environment', () => {
    const name = `CLINK_ENV_FILES_TEST_${process.pid}`;
    const cwd = directory({ '.lenv': `${name}: from-lenv\n`, '.env': `${name}=from-env\n` });
    const previous = process.cwd();
    process.chdir(cwd);
    try {
      loadEnvironmentFiles();
      assert.equal(process.env[name], 'from-lenv');
    } finally {
      process.chdir(previous);
      delete process.env[name];
    }
  });

  it('runs from main before the command line is parsed', async () => {
    const cwd = directory({ '.env': 'CLINK_ENV_FILES_MAIN=1\n' });
    const env = {};
    const chunks = [];
    const stdout = { write: (chunk) => chunks.push(String(chunk)) };
    assert.equal(await main(['--version'], { cwd, env, stdout }), 0);
    assert.deepEqual(env, { CLINK_ENV_FILES_MAIN: '1' });
    assert.ok(chunks.join('').length > 0);
  });
});
