import { describe, it, before, after } from 'node:test'
import assert from 'node:assert/strict'
import { createServer } from 'node:net'
import { findPidByPort, kill, killByPort } from './index.js'

const TEST_PORT = 49152 + Math.floor(Math.random() * 1000)

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

describe('kill', () => {
  it('returns false for a non-existent PID', () => {
    assert.equal(kill(9999999, false), false)
  })

  it('returns false for PID 1 without sufficient privileges', () => {
    // PID 1 is init/systemd; non-root cannot kill it
    const result = kill(1, false)
    assert.equal(result, false)
  })
})

describe('killByPort', () => {
  it('returns null when no process is on the port', () => {
    assert.equal(killByPort(59998, false), null)
  })
})

describe('API surface', () => {
  it('exports all three functions', () => {
    assert.equal(typeof findPidByPort, 'function')
    assert.equal(typeof kill, 'function')
    assert.equal(typeof killByPort, 'function')
  })
})
