// Store archive, a whole links store in binary links notation: ports
// rust/src/protocol/archive.rs (`export_store`, `export_store_file`,
// `import_store`, `import_store_file`).
//
// An archive is two links packets, one after the other:
//
// 1. Links, without external references: the store link `(a: s t)` is the
//    doublet `s t` at address `a`. Unused addresses are holes, so every link
//    keeps its address.
// 2. Names, with external references: one link per named store link, in
//    address order, holding the external values `address code-point…`.
//
// Both packets use packed widths. `importStore` writes every link back at its
// own address and restores the names, so exporting the imported store gives
// the same bytes again.
import { readFileSync, writeFileSync } from 'node:fs';
import { LinkError } from './errors.js';
import {
  BufferSource,
  DecodeLimits,
  LinksPacket,
  external,
  externalValue,
  isExternal,
} from './protocol-binary-packet.js';
import { ProtocolError } from './protocol-errors.js';
import { updateLink } from './lino-database-input.js';

const U32_MAX = 0xffffffffn;

/** The archive of every link and name of `storage`, as bytes. */
export function exportStore(storage) {
  const links = [...storage.allLinks()].sort((a, b) => a.index - b.index);
  const doublets = links.map((value) => [
    BigInt(value.index),
    [BigInt(value.source), BigInt(value.target)],
  ]);
  const names = [];
  for (const value of links) {
    const name = storage.getName(value.index);
    if (name != null) {
      const references = [external(value.index)];
      for (const character of name) references.push(external(character.codePointAt(0)));
      names.push([BigInt(names.length + 1), references]);
    }
  }
  return Buffer.concat([
    LinksPacket.pack(false, doublets, true).toBytes(),
    LinksPacket.pack(true, names, true).toBytes(),
  ]);
}

/** Writes the archive of `storage` to the file at `path`. */
export function exportStoreFile(storage, path) {
  const bytes = exportStore(storage);
  try {
    writeFileSync(path, bytes);
  } catch (error) {
    throw new LinkError(`Failed to write the store archive: ${path}`, { cause: error });
  }
}

/** Reads an archive into `storage`: each link at its own address, then every name. */
export function importStore(storage, bytes) {
  const limits = DecodeLimits.unlimited();
  const reader = new BufferSource(bytes);
  const links = readPacket(reader, limits, 'links');
  const names = readPacket(reader, limits, 'names');
  if (reader.offset < bytes.length) {
    throw ProtocolError.malformed('trailing bytes after the store archive');
  }
  const doublets = [...links.links()].map(([address, references]) => {
    if (references.length === 2 && !references.some(isExternal)) {
      return [storeAddress(address), storeAddress(references[0]), storeAddress(references[1])];
    }
    throw ProtocolError.malformed(`archive link ${address} is not a doublet of link addresses`);
  });
  const named = [...names.links()].map(([, references]) => decodeName(references));
  // Every address exists before any link refers to it.
  for (const [index] of doublets) {
    if (!storage.exists(index)) storage.tryEnsureCreated(index);
  }
  for (const [index, source, target] of doublets) updateLink(storage, index, source, target);
  for (const [index, name] of named) storage.setName(index, name);
  storage.save();
}

/** Reads the archive in the file at `path` into `storage`. */
export function importStoreFile(storage, path) {
  let bytes;
  try {
    bytes = readFileSync(path);
  } catch (error) {
    throw new LinkError(`Failed to read the store archive: ${path}`, { cause: error });
  }
  importStore(storage, bytes);
}

function readPacket(reader, limits, part) {
  const packet = reader.run(LinksPacket.reader(limits));
  if (packet === null) {
    throw ProtocolError.malformed(`the store archive ends before its ${part}`);
  }
  return packet;
}

function storeAddress(address) {
  if (address > U32_MAX) {
    throw ProtocolError.malformed(`address ${address} does not fit a 32-bit store`);
  }
  return Number(address);
}

function decodeName(references) {
  const values = references.map((reference) => {
    if (!isExternal(reference)) {
      throw ProtocolError.malformed('an archive name holds only external values');
    }
    return externalValue(reference);
  });
  // A packet link holds at least one reference: arity 0 is malformed.
  let name = '';
  for (const codePoint of values.slice(1)) {
    if (codePoint > 0x10ffffn || (codePoint >= 0xd800n && codePoint <= 0xdfffn)) {
      throw ProtocolError.malformed(`invalid code point ${codePoint}`);
    }
    name += String.fromCodePoint(Number(codePoint));
  }
  return [storeAddress(values[0]), name];
}
