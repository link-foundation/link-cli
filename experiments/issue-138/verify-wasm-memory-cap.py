"""Verify the actual coverage modules retain the configured linear-memory cap."""
from pathlib import Path

root = Path(__file__).resolve().parents[2]
modules = root / 'rust/wasm/target/llvm-cov-target/wasm32-unknown-unknown/debug/deps'

def uleb(stream):
    result = shift = 0
    for _ in range(10):
        raw = stream.read(1)
        assert raw, 'truncated WebAssembly integer'
        value = raw[0]
        result |= (value & 127) << shift
        if value < 128:
            return result
        shift += 7
    raise AssertionError('invalid WebAssembly integer')

for name in ['web', 'engine', 'group_commit', 'core', 'protocol', 'storage']:
    module = max(modules.glob(name + '-*.wasm'), key=lambda p: p.stat().st_mtime)
    checked = False
    with module.open('rb') as stream:
        assert stream.read(8) == b'\0asm\x01\0\0\0', module
        while section := stream.read(1):
            size = uleb(stream)
            end = stream.tell() + size
            if section[0] == 5:
                assert uleb(stream) == 1, 'expected one defined memory'
                assert uleb(stream) == 1, 'expected 32-bit non-shared bounded memory'
                minimum, maximum = uleb(stream), uleb(stream)
                assert minimum <= maximum == 4096, (module, minimum, maximum)
                assert stream.tell() == end
                checked = True
                print(f'{name}: minimum {minimum} pages; maximum {maximum} pages / 256 MiB')
                break
            stream.seek(end)
    assert checked, module
