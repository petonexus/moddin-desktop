import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createUpdaterManifest, validateReleaseVersion } from './release-artifacts.mjs'

const input = {
  version: '0.1.0-beta.6', tag: 'v0.1.0-beta.6', repository: 'petonexus/moddin-desktop',
  installer: 'Moddin Desktop_0.1.0-beta.6_x64-setup.exe', signature: 'signature\n', notes: 'Release notes',
}

test('manifest uses the GitHub asset name after spaces become dots, preserving prerelease and signature', () => {
  const manifest = createUpdaterManifest(input)
  assert.equal(manifest.version, input.version)
  assert.equal(manifest.platforms['windows-x86_64'].signature, 'signature')
  assert.equal(manifest.platforms['windows-x86_64'].url,
    'https://github.com/petonexus/moddin-desktop/releases/download/v0.1.0-beta.6/Moddin.Desktop_0.1.0-beta.6_x64-setup.exe')
})

test('rejects a tag mismatch, unsigned artifact, and legacy zip', () => {
  assert.throws(() => createUpdaterManifest({ ...input, tag: 'v0.1.0' }), /tag/)
  assert.throws(() => createUpdaterManifest({ ...input, signature: '  ' }), /signature/)
  assert.throws(() => createUpdaterManifest({ ...input, installer: 'app.nsis.zip' }), /executable/)
})

test('project versions match the beta tag and reject a different tag', () => {
  const version = JSON.parse(readFileSync('package.json', 'utf8')).version
  assert.equal(validateReleaseVersion(process.cwd(), `v${version}`), version)
  assert.throws(() => validateReleaseVersion(process.cwd(), 'v0.0.0-invalid'), /mismatch/)
})
