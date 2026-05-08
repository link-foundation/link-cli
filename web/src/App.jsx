import initClink, { Clink } from '../pkg/clink_wasm.js';
import { Link, LinksConstants, UnitedLinks } from 'doublets-web';
import {
  Activity,
  Database,
  FileClock,
  GitBranch,
  Play,
  RefreshCw,
  RotateCcw,
  Server,
  SquareTerminal,
} from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

const DOUBLETS_WEB_VERSION = '0.1.2';

const defaultQuery = '() ((child: father mother))';

const samples = [
  { label: 'Create family links', query: '() ((child: father mother))' },
  { label: 'Read all links', query: '((($i: $s $t)) (($i: $s $t)))' },
  { label: 'Swap parents', query: '((($id: father mother)) (($id: mother father)))' },
  { label: 'Delete child', query: '((child: mother father)) ()' },
];

const story = [
  {
    title: 'CLI',
    text: 'link-cli starts as a compact LiNo command: one substitution expression can create, read, update, or delete links.',
  },
  {
    title: 'Rust',
    text: 'The browser wrapper calls the same Rust query processor used by the native clink binary, with in-memory storage for the page session.',
  },
  {
    title: 'Web',
    text: 'React renders the workbench and mirrors the current link set into doublets-web, the WebAssembly package built from doublets-rs.',
  },
  {
    title: 'Pages',
    text: 'GitHub Pages publishes the static bundle: JavaScript, React assets, Rust WASM, and doublets-web WASM.',
  },
];

function App() {
  const clinkRef = useRef(null);
  const doubletsRef = useRef(null);
  const [readyState, setReadyState] = useState({
    phase: 'loading',
    wasmVersion: '',
    rustVersion: '',
    message: 'Booting WebAssembly runtimes',
  });
  const [doubletsState, setDoubletsState] = useState({
    phase: 'loading',
    count: 0,
    message: 'Preparing doublets-web',
  });
  const [query, setQuery] = useState(defaultQuery);
  const [options, setOptions] = useState({
    before: false,
    changes: true,
    after: true,
    autoCreateMissingReferences: true,
    trace: false,
  });
  const [output, setOutput] = useState('');
  const [links, setLinks] = useState([]);
  const [activeSample, setActiveSample] = useState(samples[0].label);

  const syncDoublets = useCallback((snapshot) => {
    try {
      const runtime = createDoubletsRuntime(snapshot);
      disposeDoubletsRuntime(doubletsRef.current);
      doubletsRef.current = runtime;
      setDoubletsState({
        phase: 'ready',
        count: runtime.count,
        message: `UnitedLinks mirror: ${runtime.count} stored link${runtime.count === 1 ? '' : 's'}`,
      });
    } catch (error) {
      setDoubletsState({
        phase: 'error',
        count: 0,
        message: error instanceof Error ? error.message : String(error),
      });
    }
  }, []);

  useEffect(() => {
    let cancelled = false;

    async function boot() {
      try {
        await initClink({ module_or_path: new URL('../pkg/clink_wasm_bg.wasm', import.meta.url) });
        if (cancelled) {
          return;
        }

        const clink = new Clink();
        clinkRef.current = clink;
        const snapshot = JSON.parse(clink.snapshot());
        setLinks(snapshot.links);
        setOutput(snapshot.output);
        setReadyState({
          phase: 'ready',
          wasmVersion: Clink.version(),
          rustVersion: Clink.rustCoreVersion(),
          message: 'Rust query processor online',
        });
        syncDoublets(snapshot.links);
      } catch (error) {
        if (!cancelled) {
          setReadyState({
            phase: 'error',
            wasmVersion: '',
            rustVersion: '',
            message: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    boot();

    return () => {
      cancelled = true;
      disposeDoubletsRuntime(doubletsRef.current);
      doubletsRef.current = null;
      if (clinkRef.current) {
        clinkRef.current.free?.();
        clinkRef.current = null;
      }
    };
  }, [syncDoublets]);

  const runQuery = useCallback(() => {
    if (!clinkRef.current) {
      return;
    }

    const raw = clinkRef.current.execute(query, JSON.stringify(options));
    const result = JSON.parse(raw);
    setOutput(result.output || result.error || '');
    setLinks(result.links || []);
    syncDoublets(result.links || []);
  }, [options, query, syncDoublets]);

  const resetSession = useCallback(() => {
    if (!clinkRef.current) {
      return;
    }

    const result = JSON.parse(clinkRef.current.reset());
    setOutput(result.output || '');
    setLinks(result.links || []);
    syncDoublets(result.links || []);
  }, [syncDoublets]);

  const stats = useMemo(() => {
    const references = new Set();
    for (const link of links) {
      references.add(link.id);
      references.add(link.source);
      references.add(link.target);
    }

    return {
      links: links.length,
      references: references.size,
      named: links.filter((link) => link.name).length,
    };
  }, [links]);

  return (
    <main className="appShell">
      <header className="topBar">
        <div>
          <p className="eyebrow">link-cli</p>
          <h1>WebAssembly Workbench</h1>
        </div>
        <div className="runtimeStrip" aria-label="Runtime status">
          <RuntimeBadge icon={SquareTerminal} label="Rust WASM" state={readyState.phase} />
          <RuntimeBadge icon={Database} label="doublets-web" state={doubletsState.phase} />
        </div>
      </header>

      <section className="workspace">
        <div className="queryPanel">
          <div className="panelHeader">
            <div>
              <p className="eyebrow">LiNo query</p>
              <h2>Substitution</h2>
            </div>
            <button className="iconButton" type="button" onClick={resetSession} title="Reset session">
              <RotateCcw size={18} />
            </button>
          </div>

          <textarea
            className="queryInput"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            spellCheck="false"
          />

          <div className="sampleRail" aria-label="Sample queries">
            {samples.map((sample) => (
              <button
                key={sample.label}
                className={sample.label === activeSample ? 'sampleButton active' : 'sampleButton'}
                type="button"
                onClick={() => {
                  setQuery(sample.query);
                  setActiveSample(sample.label);
                }}
              >
                {sample.label}
              </button>
            ))}
          </div>

          <div className="optionGrid">
            <Toggle label="Before" checked={options.before} onChange={() => toggleOption(setOptions, 'before')} />
            <Toggle label="Changes" checked={options.changes} onChange={() => toggleOption(setOptions, 'changes')} />
            <Toggle label="After" checked={options.after} onChange={() => toggleOption(setOptions, 'after')} />
            <Toggle
              label="Auto-create"
              checked={options.autoCreateMissingReferences}
              onChange={() => toggleOption(setOptions, 'autoCreateMissingReferences')}
            />
            <Toggle label="Trace" checked={options.trace} onChange={() => toggleOption(setOptions, 'trace')} />
          </div>

          <button
            className="primaryButton"
            type="button"
            disabled={readyState.phase !== 'ready'}
            onClick={runQuery}
          >
            <Play size={18} />
            Run query
          </button>
        </div>

        <div className="visualPanel">
          <div className="panelHeader">
            <div>
              <p className="eyebrow">Links data storage</p>
              <h2>Graph</h2>
            </div>
            <div className="counterStack">
              <Metric value={stats.links} label="links" />
              <Metric value={stats.references} label="refs" />
              <Metric value={stats.named} label="named" />
            </div>
          </div>
          <LinkGraph links={links} />
        </div>
      </section>

      <section className="detailGrid">
        <div className="resultPanel">
          <div className="panelHeader">
            <div>
              <p className="eyebrow">Output</p>
              <h2>LiNo</h2>
            </div>
            <Activity size={18} aria-hidden="true" />
          </div>
          <pre className="outputBox">{output || 'No output yet.'}</pre>
        </div>

        <div className="runtimePanel">
          <div className="panelHeader">
            <div>
              <p className="eyebrow">Runtime</p>
              <h2>Status</h2>
            </div>
            <RefreshCw size={18} aria-hidden="true" />
          </div>
          <StatusRow icon={Server} label="Rust wrapper" value={readyState.message} />
          <StatusRow icon={GitBranch} label="Rust core" value={readyState.rustVersion || 'pending'} />
          <StatusRow icon={Database} label="doublets-web" value={`${DOUBLETS_WEB_VERSION} / ${doubletsState.message}`} />
          <StatusRow icon={FileClock} label="Session" value={`${stats.links} link snapshots mirrored`} />
        </div>
      </section>

      <section className="storyBand" aria-label="link-cli story">
        {story.map((item) => (
          <article className="storyItem" key={item.title}>
            <span>{item.title}</span>
            <p>{item.text}</p>
          </article>
        ))}
      </section>
    </main>
  );
}

function RuntimeBadge({ icon: Icon, label, state }) {
  return (
    <div className={`runtimeBadge ${state}`}>
      <Icon size={16} />
      <span>{label}</span>
    </div>
  );
}

function Toggle({ label, checked, onChange }) {
  return (
    <label className="toggleControl">
      <input type="checkbox" checked={checked} onChange={onChange} />
      <span>{label}</span>
    </label>
  );
}

function Metric({ value, label }) {
  return (
    <div className="metric">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function StatusRow({ icon: Icon, label, value }) {
  return (
    <div className="statusRow">
      <Icon size={17} />
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function LinkGraph({ links }) {
  const graph = useMemo(() => buildGraph(links), [links]);

  if (graph.nodes.length === 0) {
    return (
      <div className="emptyGraph">
        <Database size={32} />
      </div>
    );
  }

  return (
    <svg className="graphCanvas" viewBox="0 0 760 420" role="img" aria-label="Current links graph">
      <defs>
        <marker id="arrowTarget" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto">
          <path d="M0,0 L8,4 L0,8 Z" className="edgeArrow" />
        </marker>
      </defs>
      <rect x="0" y="0" width="760" height="420" rx="8" className="graphBackground" />
      {graph.edges.map((edge) => {
        if (edge.self) {
          const node = graph.points.get(edge.from);
          return (
            <circle
              key={edge.key}
              cx={node.x}
              cy={node.y - 18}
              r="18"
              className={`edgeLine ${edge.kind}`}
              fill="none"
            />
          );
        }
        const from = graph.points.get(edge.from);
        const to = graph.points.get(edge.to);
        return (
          <line
            key={edge.key}
            x1={from.x}
            y1={from.y}
            x2={to.x}
            y2={to.y}
            className={`edgeLine ${edge.kind}`}
            markerEnd="url(#arrowTarget)"
          />
        );
      })}
      {graph.nodes.map((node) => (
        <g key={node.id} transform={`translate(${node.x} ${node.y})`}>
          <circle className={node.named ? 'node named' : 'node'} r="28" />
          <text className="nodeLabel" y="-2">
            {node.label}
          </text>
          <text className="nodeSubLabel" y="14">
            #{node.id}
          </text>
        </g>
      ))}
    </svg>
  );
}

function buildGraph(links) {
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

function toggleOption(setOptions, key) {
  setOptions((current) => ({ ...current, [key]: !current[key] }));
}

function createDoubletsRuntime(snapshot) {
  const links = new UnitedLinks(new LinksConstants());
  const ordered = [...snapshot].sort((a, b) => a.id - b.id);
  let lastCreated = 0;

  for (const item of ordered) {
    while (lastCreated < item.id) {
      lastCreated = links.create();
    }
    links.update(item.id, item.source, item.target);
  }

  const count = countDoublets(links);
  return { links, count };
}

function countDoublets(links) {
  const constants = links.constants;
  try {
    return links.count(new Link(constants.any, constants.any, constants.any));
  } finally {
    constants.free?.();
  }
}

function disposeDoubletsRuntime(runtime) {
  if (!runtime) {
    return;
  }
  runtime.links?.free?.();
}

export default App;
