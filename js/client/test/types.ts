import { LinksClient, LinksDecorator, createWasmClient, ANY, type Links, UnionLinks, RemapLinks, OffsetAddressMap, AddressTable, ExternalIdMap, LinksExternalIds, NotFoundError } from '../types/index.js';
import { connectTcp } from '../types/tcp.js';
import { connectProcess } from '../types/process.js';
class Logging extends LinksDecorator {
  override async create(source: bigint | number, target: bigint | number) { return super.create(source, target); }
}
async function main(links: Links) {
  const link = await links.create(1n, 1);
  const n: bigint = link ? await links.count([ANY, link.index]) : 0n;
  const r = await links.batch([{ op: 'create', source: 1, target: 2 }], { timeout: 1000 });
  if (!r.completed) throw r.firstError;
  const c = await connectTcp('127.0.0.1:1', { verify: true, deadline: new Date() });
  const p = await connectProcess({ command: 'clink', db: 'a b.links' });
  const w = createWasmClient({ request: (t: string) => t });
  new Logging(w).require('rawTcp');
  const subscription: number = await c.subscribe([ANY, ANY, ANY]);
  const stop = c.onNotification(({ subscription: n, changes }) => void [n, changes[0]?.after?.index]);
  stop();
  const note = await c.receiveNotification({ timeout: 10 });
  if (note) for (const { before, after } of note.changes) void [before?.source, after?.target];
  await c.unsubscribe(subscription);
  void c.pendingNotifications;
  const point = await c.createTripletPoint();
  const triplet = await c.createTriplet(point.index, 0, 0n);
  const typed: bigint = await c.countTriplets([ANY, triplet.type, triplet.from, triplet.to]);
  for (const { before, after } of await c.updateTriplet(triplet.index, point.index, 0, 0)) void [before?.type, after?.to];
  const named = await c.queryTriplets('() ((1 0 0))');
  void [typed, named[0]?.after?.from.length, (await c.eachTriplets())[0]?.from, (await c.exportTriplets())[0]?.type.length, (await c.deleteTriplet(1))[0]?.before];
  await c.subscribeTriplets([ANY, point.index]);
  const tripletNote = await c.receiveTripletNotification({ timeout: 10 });
  if (tripletNote) for (const { after } of tripletNote.changes) void after?.type;
  c.onTripletNotification(({ changes }) => void changes[0]?.after?.from)();
  await c.batch([{ op: 'createTriplet', type: 1, from: 0, to: 0 }, { op: 'eachTriplets', restriction: [ANY, 1, 0, 0] }]);
  const union: Links = new UnionLinks([w, new RemapLinks(c, new OffsetAddressMap(1n << 24n))]);
  const owner: number | null = await (union as UnionLinks).ownerOf(1n);
  const table = new AddressTable<string>({ newId: () => crypto.randomUUID() });
  const uuids = new RemapLinks(w, table);
  const uuid: string | bigint = await uuids.toOuter(1n);
  const pairs: [string, bigint][] = table.pairs();
  const kept = new RemapLinks(w, new ExternalIdMap(p, 'uuid'));
  const bound: bigint | null = await new LinksExternalIds('uuid').find(p, 'id');
  void [owner, uuid, pairs, kept, bound, new AddressTable({ first: 500n }), NotFoundError];
  await using x = c;
  return [n, p];
}
