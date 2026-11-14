import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { constants } from 'node:os';
import { describe, it } from 'node:test';
import {
  errorChain,
  errorText,
  formatErrorReport,
  ioErrorText,
  isSystemError,
  reportError,
} from '../src/error-report.js';
import { ParseError } from '../src/errors.js';

function systemError(call) {
  try {
    call();
  } catch (error) {
    return error;
  }
  throw new Error('expected a system error');
}

const missing = () => systemError(() => readFileSync('/nonexistent/clink/missing.lino'));

describe('error reports', () => {
  it('prints a Node.js system error as the Rust std::io::Error', () => {
    const error = missing();
    assert.equal(isSystemError(error), true);
    assert.equal(
      ioErrorText(error),
      `No such file or directory (os error ${constants.errno.ENOENT})`,
    );
    assert.equal(errorText(error), ioErrorText(error));
  });

  it('keeps the Node.js text and errno of a code the table does not know', () => {
    const error = Object.assign(new Error("EWEIRD: something odd happened, open 'x'"), {
      code: 'EWEIRD',
      errno: -4242,
    });
    assert.equal(ioErrorText(error, {}), 'something odd happened (os error 4242)');
    assert.equal(
      ioErrorText(Object.assign(new Error('EWEIRD: odd'), { code: 'EWEIRD', errno: -7 }), {
        EWEIRD: 9,
      }),
      'odd (os error 9)',
    );
  });

  it('is not fooled by errors that only look like system errors', () => {
    assert.equal(isSystemError(Object.assign(new Error('x'), { code: 'ERR_X' })), false);
    assert.equal(isSystemError(Object.assign(new Error('x'), { code: 'ENOENT' })), false);
    assert.equal(isSystemError({ code: 'ENOENT', errno: -2 }), false);
    assert.equal(errorText('plain text'), 'plain text');
  });

  it('prints a lone error without a cause section', () => {
    assert.equal(formatErrorReport(new Error('unknown option --bogus')), 'unknown option --bogus');
  });

  it('prints one cause indented by four spaces, as anyhow does', () => {
    const error = new Error('Failed to read LiNo input: missing.lino', { cause: missing() });
    assert.equal(
      formatErrorReport(error),
      'Failed to read LiNo input: missing.lino\n\nCaused by:\n' +
        `    No such file or directory (os error ${constants.errno.ENOENT})`,
    );
  });

  it('numbers several causes and aligns their continuation lines', () => {
    const error = new Error('top', {
      cause: new Error('middle\nsecond line', { cause: new Error('bottom') }),
    });
    assert.equal(
      formatErrorReport(error),
      'top\n\nCaused by:\n    0: middle\n       second line\n    1: bottom',
    );
    assert.equal(
      formatErrorReport(new Error('top', { cause: new Error('a\nb') })),
      'top\n\nCaused by:\n    a\n    b',
    );
  });

  it('prints a multi-line message as it is', () => {
    const error = new ParseError('Syntax error at line 1, column 7\n1 | (( 1 1\n  |       ^');
    assert.equal(
      formatErrorReport(error),
      'Parse error: Syntax error at line 1, column 7\n1 | (( 1 1\n  |       ^',
    );
  });

  it('stops at a cause cycle and reports non-error causes', () => {
    const error = new Error('loop');
    error.cause = error;
    assert.deepEqual(errorChain(error), [error]);
    assert.equal(
      formatErrorReport(new Error('top', { cause: 'text cause' })),
      'top\n\nCaused by:\n    text cause',
    );
  });

  it('writes `Error: ` and the report to standard error', () => {
    const written = [];
    reportError(new Error('boom'), { write: (text) => written.push(text) });
    assert.deepEqual(written, ['Error: boom\n']);
  });
});
