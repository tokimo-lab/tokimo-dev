import { existsSync } from 'node:fs'
import { execSync } from 'node:child_process'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform, arch } from 'node:process'

const __dirname = dirname(fileURLToPath(import.meta.url))
const pkgRoot = join(__dirname, '..')

const TARGET_MAP = {
  'win32-x64': 'win32-x64-msvc',
  'darwin-x64': 'darwin-x64',
  'darwin-arm64': 'darwin-arm64',
  'linux-x64': 'linux-x64-gnu',
}

const target = TARGET_MAP[`${platform}-${arch}`]
if (!target) {
  console.log(`[port-ops] No prebuilt target for ${platform}-${arch}, skipping`)
  process.exit(0)
}

if (existsSync(join(pkgRoot, `port-ops.${target}.node`))) {
  process.exit(0)
}

console.log(`[port-ops] Building native addon for ${target}...`)
try {
  execSync('npx napi build --platform --release', { cwd: pkgRoot, stdio: 'inherit' })
  console.log('[port-ops] Build succeeded')
} catch {
  console.warn('[port-ops] Build failed — native addon unavailable')
}
