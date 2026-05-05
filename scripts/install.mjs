import { existsSync, readdirSync } from 'node:fs'
import { execSync } from 'node:child_process'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform, arch } from 'node:process'
import { renameSync } from 'node:fs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const pkgRoot = join(__dirname, '..')

const TARGET_MAP = {
  'win32-x64': { napi: 'win32-x64-msvc', ext: 'dll' },
  'darwin-x64': { napi: 'darwin-x64', ext: 'dylib' },
  'darwin-arm64': { napi: 'darwin-arm64', ext: 'dylib' },
  'linux-x64': { napi: 'linux-x64-gnu', ext: 'so' },
  'linux-arm64': { napi: 'linux-arm64-gnu', ext: 'so' },
}

const target = TARGET_MAP[`${platform}-${arch}`]
if (!target) {
  console.log(`[port-ops] No target for ${platform}-${arch}, skipping`)
  process.exit(0)
}

const binaryPath = join(pkgRoot, `port-ops.${target.napi}.node`)
if (existsSync(binaryPath)) {
  process.exit(0)
}

console.log(`[port-ops] Building native addon for ${target.napi}...`)
try {
  execSync('cargo build --release', { cwd: pkgRoot, stdio: 'inherit' })

  // Find and rename the built library
  const releaseDir = join(pkgRoot, 'target', 'release')
  const builtName = `tokimo_dev.${target.ext}`
  const builtPath = join(releaseDir, builtName)
  if (!existsSync(builtPath)) {
    // Try .node extension (some napi-build configs)
    const alt = join(releaseDir, 'tokimo_dev.node')
    if (existsSync(alt)) {
      renameSync(alt, binaryPath)
    } else {
      // Search for any matching file
      const found = readdirSync(releaseDir).find(f => f.startsWith('tokimo_dev'))
      if (found) renameSync(join(releaseDir, found), binaryPath)
      else throw new Error(`Built binary not found in ${releaseDir}`)
    }
  } else {
    renameSync(builtPath, binaryPath)
  }

  console.log('[port-ops] Build succeeded')
} catch (e) {
  console.warn('[port-ops] Build failed:', e.message)
}
