import { describe, expect, test } from 'bun:test';
import { createServer } from 'node:http';

import {
  DEFAULT_MAX_ATTEMPTS,
  DEFAULT_SLEEP_SECONDS,
  checkNugetPackageVersion,
  createNugetNuspecUrl,
  parseArgs,
  waitForNugetPackage,
} from './wait-for-nuget.mjs';

/**
 * Start a 127.0.0.1 mock that serves the NuGet flat-container nuspec endpoint
 * `wait-for-nuget.mjs` polls. The script honours `--flat-container-url` (and
 * `NUGET_FLAT_CONTAINER_URL` / `NUGET_INDEX_URL`) overrides so we can point the
 * probe at the mock without making real HTTP requests.
 *
 * @param {{ statuses: number[] }} options Sequence of HTTP statuses to serve in
 *   order. The last entry is reused once exhausted.
 */
function startNugetMock({ statuses }) {
  const sockets = new Set();
  const served = [];
  let index = 0;
  const server = createServer((req, res) => {
    res.setHeader('connection', 'close');
    const status = statuses[Math.min(index, statuses.length - 1)];
    index += 1;
    served.push({ url: req.url, method: req.method, status });
    res.writeHead(status).end();
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });

  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({
        flatContainerUrl: `http://127.0.0.1:${port}`,
        served,
        close: () =>
          new Promise((r) => {
            for (const socket of sockets) {
              socket.destroy();
            }
            server.close(() => r());
          }),
      });
    });
  });
}

describe('wait-for-nuget parseArgs()', () => {
  test('defaults to two-minute checks across the NuGet indexing window', () => {
    const config = parseArgs(
      ['--package-id', 'MyPackage', '--release-version', '2.4.0'],
      {}
    );

    expect(config.packageId).toBe('MyPackage');
    expect(config.releaseVersion).toBe('2.4.0');
    expect(config.maxAttempts).toBe(DEFAULT_MAX_ATTEMPTS);
    expect(config.sleepSeconds).toBe(DEFAULT_SLEEP_SECONDS);
    expect(DEFAULT_MAX_ATTEMPTS).toBe(8);
    expect(DEFAULT_SLEEP_SECONDS).toBe(120);
  });

  test('accepts --version as an alias for --release-version', () => {
    const config = parseArgs(
      ['--package-id', 'MyPackage', '--version', '0.2.1'],
      {}
    );

    expect(config.releaseVersion).toBe('0.2.1');
  });

  test('honours CLI flags for max-attempts, sleep-seconds and flat-container-url', () => {
    const config = parseArgs(
      [
        '--package-id',
        'MyPackage',
        '--release-version',
        '1.0.0',
        '--max-attempts',
        '3',
        '--sleep-seconds',
        '5',
        '--flat-container-url',
        'http://127.0.0.1:9999',
      ],
      {}
    );

    expect(config.maxAttempts).toBe(3);
    expect(config.sleepSeconds).toBe(5);
    expect(config.flatContainerUrl).toBe('http://127.0.0.1:9999');
  });

  test('reads overrides from environment variables', () => {
    const config = parseArgs([], {
      PACKAGE_ID: 'MyPackage',
      RELEASE_VERSION: '1.2.3',
      NUGET_WAIT_MAX_ATTEMPTS: '4',
      NUGET_WAIT_SLEEP_SECONDS: '7',
      NUGET_FLAT_CONTAINER_URL: 'http://mock/flat',
    });

    expect(config.packageId).toBe('MyPackage');
    expect(config.releaseVersion).toBe('1.2.3');
    expect(config.maxAttempts).toBe(4);
    expect(config.sleepSeconds).toBe(7);
    expect(config.flatContainerUrl).toBe('http://mock/flat');
  });

  test('inline --flag=value form is supported', () => {
    const config = parseArgs(
      ['--package-id=MyPackage', '--release-version=2.0.0', '--max-attempts=2'],
      {}
    );

    expect(config.packageId).toBe('MyPackage');
    expect(config.releaseVersion).toBe('2.0.0');
    expect(config.maxAttempts).toBe(2);
  });

  test('rejects non-positive integers for --max-attempts', () => {
    expect(() =>
      parseArgs(
        ['--package-id', 'p', '--release-version', '1.0.0', '--max-attempts', '0'],
        {}
      )
    ).toThrow(/--max-attempts must be a positive integer/);
  });

  test('rejects non-positive integers for --sleep-seconds', () => {
    expect(() =>
      parseArgs(
        ['--package-id', 'p', '--release-version', '1.0.0', '--sleep-seconds', '-1'],
        {}
      )
    ).toThrow(/--sleep-seconds must be a positive integer/);
  });

  test('throws when a flag is missing its value', () => {
    expect(() => parseArgs(['--package-id'], {})).toThrow(/Missing value for --package-id/);
  });
});

describe('wait-for-nuget createNugetNuspecUrl()', () => {
  test('builds the flat-container nuspec URL with lowercased id and version', () => {
    expect(
      createNugetNuspecUrl({
        flatContainerUrl: 'https://api.nuget.org/v3-flatcontainer/',
        packageId: 'MyPackage',
        version: '2.4.0',
      })
    ).toBe('https://api.nuget.org/v3-flatcontainer/mypackage/2.4.0/mypackage.nuspec');
  });

  test('lowercases mixed-case versions (e.g. semver pre-release tags)', () => {
    expect(
      createNugetNuspecUrl({
        flatContainerUrl: 'https://api.nuget.org/v3-flatcontainer',
        packageId: 'MyPackage',
        version: '1.0.0-RC.1',
      })
    ).toBe('https://api.nuget.org/v3-flatcontainer/mypackage/1.0.0-rc.1/mypackage.nuspec');
  });

  test('defaults to the official flat-container URL when not specified', () => {
    expect(
      createNugetNuspecUrl({ packageId: 'MyPackage', version: '1.0.0' })
    ).toBe('https://api.nuget.org/v3-flatcontainer/mypackage/1.0.0/mypackage.nuspec');
  });
});

describe('wait-for-nuget waitForNugetPackage()', () => {
  test('succeeds when indexing takes longer than the old 125-second loop', async () => {
    let attempts = 0;
    const sleeps = [];

    const available = await waitForNugetPackage({
      checkAvailability: async () => {
        attempts++;
        return {
          available: attempts === 8,
          status: attempts === 8 ? 200 : 404,
        };
      },
      maxAttempts: 8,
      packageId: 'MyPackage',
      sleepFn: async (seconds) => {
        sleeps.push(seconds);
      },
      sleepSeconds: 120,
      stdout: () => {},
      version: '2.4.0',
    });

    expect(available).toBe(true);
    expect(attempts).toBe(8);
    expect(sleeps).toEqual([120, 120, 120, 120, 120, 120, 120]);
  });

  test('fails only after exhausting all attempts', async () => {
    let attempts = 0;
    const sleeps = [];

    const available = await waitForNugetPackage({
      checkAvailability: async () => {
        attempts++;
        return { available: false, status: 404 };
      },
      maxAttempts: 3,
      packageId: 'MyPackage',
      sleepFn: async (seconds) => {
        sleeps.push(seconds);
      },
      sleepSeconds: 120,
      stdout: () => {},
      version: '2.4.0',
    });

    expect(available).toBe(false);
    expect(attempts).toBe(3);
    expect(sleeps).toEqual([120, 120]);
  });

  test('returns immediately when the package is already indexed', async () => {
    let attempts = 0;
    const sleeps = [];

    const available = await waitForNugetPackage({
      checkAvailability: async () => {
        attempts++;
        return { available: true, status: 200 };
      },
      maxAttempts: 8,
      packageId: 'MyPackage',
      sleepFn: async (seconds) => {
        sleeps.push(seconds);
      },
      sleepSeconds: 120,
      stdout: () => {},
      version: '2.4.0',
    });

    expect(available).toBe(true);
    expect(attempts).toBe(1);
    expect(sleeps).toEqual([]);
  });

  test('tolerates transient network errors before succeeding', async () => {
    const statuses = [
      { available: false, status: 'network-error', error: 'ECONNRESET' },
      { available: true, status: 200 },
    ];
    let attempts = 0;
    const sleeps = [];

    const available = await waitForNugetPackage({
      checkAvailability: async () => statuses[attempts++],
      maxAttempts: 4,
      packageId: 'MyPackage',
      sleepFn: async (seconds) => {
        sleeps.push(seconds);
      },
      sleepSeconds: 60,
      stdout: () => {},
      version: '1.0.0',
    });

    expect(available).toBe(true);
    expect(attempts).toBe(2);
    expect(sleeps).toEqual([60]);
  });
});

describe('wait-for-nuget checkNugetPackageVersion()', () => {
  test('reports 200 responses as available against a live HTTP endpoint', async () => {
    const mock = await startNugetMock({ statuses: [200] });
    try {
      const result = await checkNugetPackageVersion({
        flatContainerUrl: mock.flatContainerUrl,
        packageId: 'MyPackage',
        version: '1.0.0',
      });
      expect(result.available).toBe(true);
      expect(result.status).toBe(200);
      expect(result.url).toBe(
        `${mock.flatContainerUrl}/mypackage/1.0.0/mypackage.nuspec`
      );
      expect(mock.served).toHaveLength(1);
      expect(mock.served[0].method).toBe('HEAD');
      expect(mock.served[0].url).toBe('/mypackage/1.0.0/mypackage.nuspec');
    } finally {
      await mock.close();
    }
  });

  test('reports 404 responses as unavailable without throwing', async () => {
    const mock = await startNugetMock({ statuses: [404] });
    try {
      const result = await checkNugetPackageVersion({
        flatContainerUrl: mock.flatContainerUrl,
        packageId: 'MyPackage',
        version: '9.9.9',
      });
      expect(result.available).toBe(false);
      expect(result.status).toBe(404);
    } finally {
      await mock.close();
    }
  });

  test('treats fetch network errors as a soft failure', async () => {
    const result = await checkNugetPackageVersion({
      fetchImpl: async () => {
        throw new Error('connect ECONNREFUSED');
      },
      flatContainerUrl: 'http://127.0.0.1:1',
      packageId: 'MyPackage',
      version: '1.0.0',
    });
    expect(result.available).toBe(false);
    expect(result.status).toBe('network-error');
    expect(result.error).toMatch(/ECONNREFUSED/);
  });

  test('end-to-end: waits through 404 responses then succeeds when the package appears', async () => {
    const mock = await startNugetMock({ statuses: [404, 404, 200] });
    const sleeps = [];

    try {
      const available = await waitForNugetPackage({
        flatContainerUrl: mock.flatContainerUrl,
        maxAttempts: 5,
        packageId: 'MyPackage',
        sleepFn: async (seconds) => {
          sleeps.push(seconds);
        },
        sleepSeconds: 1,
        stdout: () => {},
        version: '1.0.0',
      });

      expect(available).toBe(true);
      expect(mock.served).toHaveLength(3);
      expect(sleeps).toEqual([1, 1]);
    } finally {
      await mock.close();
    }
  });
});
