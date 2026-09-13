const PANEL_ID = 'moddin-debug-error'

export function showDebugErrorPanel(title: string, details: string) {
  if (typeof document === 'undefined') return

  let panel = document.getElementById(PANEL_ID)
  if (!panel) {
    panel = document.createElement('section')
    panel.id = PANEL_ID
    panel.setAttribute('role', 'alert')
    panel.style.cssText = [
      'position:fixed',
      'inset:18px',
      'z-index:99999',
      'overflow:auto',
      'padding:22px',
      'border:1px solid #8f3d4d',
      'border-radius:12px',
      'color:#f4d8dc',
      'background:#1d1015',
      'font:13px/1.5 Consolas, monospace',
      'box-shadow:0 20px 80px rgba(0,0,0,.65)',
    ].join(';')
    document.body?.appendChild(panel)
  }

  panel.innerHTML = ''
  const heading = document.createElement('strong')
  heading.textContent = `Moddin debug: ${title}`
  const message = document.createElement('p')
  message.textContent = 'A interface encontrou um erro. Os detalhes abaixo também estão em window.__MODDIN_DEBUG__.logs().'
  const pre = document.createElement('pre')
  pre.style.cssText = 'white-space:pre-wrap;margin:16px 0 0;color:#ffdfe3'
  pre.textContent = details
  panel.append(heading, message, pre)
}

export function hideDebugErrorPanel() {
  if (typeof document === 'undefined') return
  document.getElementById(PANEL_ID)?.remove()
}
