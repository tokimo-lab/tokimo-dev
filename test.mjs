import { describe, it, before, after } from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:net'
import { spawn } from 'node:child_process'
import {
  findPidByPort,
  findPidByPortAll,
  isPortAvailable,
  waitForPortFree,
  findPidsByName,
  kill,
  killTree,
  killByPort,
} from './index.js'

const TEST_PORT = 49152 + Math.floor(Math.random() * 1000)

// ── findPidByPort ─────────────────────────────────────────────────────

describe('findPidByPort', () => {
  let server

  before(() => {
    return new Promise((resolve) => {
      server = createServer()
      server.listen(TEST_PORT, '127.0.0.1', () => resolve())
    })
  })

  after(() => {
    return new Promise((resolve) => server.close(resolve))
  })

  it('returns a PID for a port that is in use', () => {
    const pid = findPidByPort(TEST_PORT)
    assert.ok(pid !== null && pid > 0, `expected a valid PID, got ${pid}`)
    assert.equal(pid, process.pid)
  })

  it('returns null for a port that is not in use', () => {
    const pid = findPidByPort(59999)
    assert.equal(pid, null)
  })
})

// ── findPidByPortAll ──────────────────────────────────────────────────

describe('findPidByPortAll', () => {
  let server

  before(() => {
    return new Promise((resolve) => {
      server = createServer()
      server.listen(TEST_PORT + 1, '127.0.0.1', () => resolve())
    })
  })

  after(() => {
    return new Promise((resolve) => server.close(resolve))
  })

  it('returns an array containing the PID', () => {
    const pids = findPidByPortAll(TEST_PORT + 1)
    assert.ok(Array.isArray(pids), 'expected an array')
    assert.ok(pids.includes(process.pid), `expected ${process.pid} in ${pids}`)
  })

  it('returns an empty array for unused port', () => {
    const pids = findPidByPortAll(59997)
    assert.deepEqual(pids, [])
  })
})

// ── isPortAvailable ───────────────────────────────────────────────────

describe('isPortAvailable', () => {
  let server

  before(() => {
    return new Promise((resolve) => {
      server = createServer()
      server.listen(TEST_PORT + 2, '127.0.0.1', () => resolve())
    })
  })

  after(() => {
    return new Promise((resolve) => server.close(resolve))
  })

  it('returns false for an occupied port', () => {
    assert.equal(isPortAvailable(TEST_PORT + 2), false)
  })

  it('returns true for a free port', () => {
    assert.equal(isPortAvailable(59996), true)
  })
})

// ── waitForPortFree ───────────────────────────────────────────────────

describe('waitForPortFree', () => {
  it('returns true immediately for a free port', () => {
    const result = waitForPortFree(59995, 1000)
    assert.equal(result, true)
  })

  it('returns false on timeout for an occupied port', (_, done) => {
    const server = createServer()
    server.listen(TEST_PORT + 3, '127.0.0.1', () => {
      const result = waitForPortFree(TEST_PORT + 3, 200)
      assert.equal(result, false)
      server.close(() => done())
    })
  })
})

// ── findPidsByName ────────────────────────────────────────────────────

describe('findPidsByName', () => {
  it('finds a spawned process by name', (_, done) => {
    const marker = 'portopstest'
    const child = spawn(process.execPath, [
      '-e',
      `process.title="${marker}"; setInterval(()=>{}, 1e9)`,
    ])
    setTimeout(() => {
      try {
        const needle = process.platform === 'win32' ? 'node' : marker
        const pids = findPidsByName(needle)
        assert.ok(Array.isArray(pids), 'expected an array')
        assert.ok(pids.includes(child.pid), `expected ${child.pid} in ${pids}`)
      } finally {
        child.kill()
        done()
      }
    }, 300)
  })

  it('returns empty for a non-existent process name', () => {
    const pids = findPidsByName('definitely_not_a_real_process_name_12345')
    assert.deepEqual(pids, [])
  })
})

// ── kill ──────────────────────────────────────────────────────────────

describe('kill', () => {
  it('returns false for a non-existent PID', () => {
    assert.equal(kill(9999999, false), false)
  })

  it('returns false for PID 1 without sufficient privileges', () => {
    const result = kill(1, false)
    assert.equal(result, false)
  })
})

// ── killTree ──────────────────────────────────────────────────────────

describe('killTree', () => {
  it('kills a process and its child', (t, done) => {
    // Spawn a child that creates a grandchild
    const child = spawn(process.execPath, [
      '-e',
      'const g = require("child_process").spawn(process.execPath, ["-e", "setInterval(()=>{}, 1e9)"]); setInterval(()=>{}, 1e9)',
    ])

    const parentPid = child.pid
    // Give child time to spawn its own child
    setTimeout(() => {
      const result = killTree(parentPid, true)
      assert.equal(result, true)

      // Verify parent is gone
      setTimeout(() => {
        assert.equal(findPidByPortAll(0).includes(parentPid) || !isProcessAlive(parentPid), true)
        done()
      }, 200)
    }, 500)
  })

  it('returns false for a non-existent PID', () => {
    assert.equal(killTree(9999999, true), false)
  })
})

// ── killByPort ────────────────────────────────────────────────────────

describe('killByPort', () => {
  it('returns null when no process is on the port', () => {
    assert.equal(killByPort(59998, false), null)
  })
})

// ── API surface ───────────────────────────────────────────────────────

describe('API surface', () => {
  it('exports all functions', () => {
    assert.equal(typeof findPidByPort, 'function')
    assert.equal(typeof findPidByPortAll, 'function')
    assert.equal(typeof isPortAvailable, 'function')
    assert.equal(typeof waitForPortFree, 'function')
    assert.equal(typeof findPidsByName, 'function')
    assert.equal(typeof kill, 'function')
    assert.equal(typeof killTree, 'function')
    assert.equal(typeof killByPort, 'function')
  })
})

// ── helpers ───────────────────────────────────────────────────────────

function isProcessAlive(pid) {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}
