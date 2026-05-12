export function buildGraph(links) {
  const ids = new Set();
  for (const link of links) {
    ids.add(link.id);
    ids.add(link.source);
    ids.add(link.target);
  }

  const ordered = Array.from(ids)
    .filter((id) => id > 0)
    .sort((a, b) => a - b);
  const names = new Map(links.map((link) => [link.id, link.name]).filter(([, name]) => name));
  const radius = Math.min(150, Math.max(90, ordered.length * 18));
  const center = { x: 380, y: 210 };
  const points = new Map();

  ordered.forEach((id, index) => {
    const angle = ordered.length === 1 ? -Math.PI / 2 : (index / ordered.length) * Math.PI * 2 - Math.PI / 2;
    points.set(id, {
      id,
      x: center.x + Math.cos(angle) * radius,
      y: center.y + Math.sin(angle) * radius,
      label: names.get(id) || String(id),
      named: names.has(id),
    });
  });

  const edges = links.flatMap((link) => [
    {
      key: `${link.id}-source`,
      from: link.source,
      to: link.id,
      kind: 'source',
      self: link.source === link.id,
    },
    {
      key: `${link.id}-target`,
      from: link.id,
      to: link.target,
      kind: 'target',
      self: link.id === link.target,
    },
  ]);

  return {
    nodes: ordered.map((id) => points.get(id)),
    points,
    edges: edges.filter((edge) => points.has(edge.from) && points.has(edge.to)),
  };
}
