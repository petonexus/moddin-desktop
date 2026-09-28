import { defineLocalizedCopy, localizedCopyFor } from '../../i18n/localizedCopy'

const ptBR = {
  button: 'IA local',
  title: 'Conectar uma IA local',
  subtitle: 'Conecte o Cursor, Claude Desktop ou Codex diretamente ao Moddin pelo protocolo MCP — sem colar prompts, sem arquivos JSON.',
  close: 'Fechar',
  refresh: 'Atualizar status',
  detecting: 'Procurando IAs instaladas…',
  intro:
    'Detectamos as IAs instaladas neste PC. Clique em "Conectar" para registrar o Moddin como servidor MCP daquela ferramenta. Depois é só abrir a IA e pedir para adicionar um mod.',
  notInstalled: 'Não instalada neste PC.',
  detectedNotConfigured: 'Instalada. Clique em Conectar para registrar o Moddin.',
  configured: 'Conectada e apontando para o servidor do Moddin.',
  configError: 'A configuração existe, mas está apontando para outro servidor.',
  connect: 'Conectar',
  disconnect: 'Desconectar',
  resourcePath: 'Servidor MCP',
  configPath: 'Arquivo de configuração',
  binaryPath: 'Binário da IA',
  banner: 'O Moddin nunca grava nada dentro das pastas dos jogos. A IA só escreve arquivos YAML em uma pasta local sua, e você aplica pelo Moddin com preview e rollback.',
  connectFailed: 'Não foi possível conectar: {error}',
  disconnectFailed: 'Não foi possível desconectar: {error}',
  installCursorHint: 'Você não tem o Cursor instalado. Baixe em cursor.com e clique em "Atualizar status" depois.',
} as const

const enUS = {
  button: 'Local AI',
  title: 'Connect a local AI',
  subtitle: 'Connect Cursor, Claude Desktop or Codex straight into Moddin via MCP. No copy-pasted prompts, no JSON files.',
  close: 'Close',
  refresh: 'Refresh status',
  detecting: 'Looking for installed AIs...',
  intro:
    'We detected the AIs installed on this PC. Click "Connect" to register Moddin as the MCP server for that tool. After that, open the AI and ask it to add a mod.',
  notInstalled: 'Not installed on this PC.',
  detectedNotConfigured: 'Installed. Click Connect to register Moddin.',
  configured: 'Connected and pointing at the Moddin server.',
  configError: 'A config entry exists, but it points elsewhere.',
  connect: 'Connect',
  disconnect: 'Disconnect',
  resourcePath: 'MCP server',
  configPath: 'Config file',
  binaryPath: 'AI binary',
  banner: 'Moddin never writes inside game folders. The AI only writes YAML files to a local folder of yours, and you apply from Moddin with preview and rollback.',
  connectFailed: 'Could not connect: {error}',
  disconnectFailed: 'Could not disconnect: {error}',
  installCursorHint: 'You do not have Cursor installed. Download it from cursor.com then click "Refresh status".',
} as const

const esES = {
  button: 'IA local',
  title: 'Conectar una IA local',
  subtitle: 'Conecta Cursor, Claude Desktop o Codex directamente a Moddin mediante el protocolo MCP: sin pegar prompts, sin archivos JSON.',
  close: 'Cerrar',
  refresh: 'Actualizar estado',
  detecting: 'Buscando IAs instaladas...',
  intro:
    'Detectamos las IAs instaladas en este PC. Haz clic en "Conectar" para registrar Moddin como servidor MCP de esa herramienta. Después abre la IA y pide añadir un mod.',
  notInstalled: 'No instalada en este PC.',
  detectedNotConfigured: 'Instalada. Haz clic en Conectar para registrar Moddin.',
  configured: 'Conectada y apuntando al servidor de Moddin.',
  configError: 'Existe una configuración, pero apunta a otro servidor.',
  connect: 'Conectar',
  disconnect: 'Desconectar',
  resourcePath: 'Servidor MCP',
  configPath: 'Archivo de configuración',
  binaryPath: 'Binario de la IA',
  banner: 'Moddin nunca escribe dentro de las carpetas de los juegos. La IA solo escribe YAML en una carpeta local tuya, y tú aplicas desde Moddin con vista previa y rollback.',
  connectFailed: 'No se pudo conectar: {error}',
  disconnectFailed: 'No se pudo desconectar: {error}',
  installCursorHint: 'No tienes Cursor instalado. Descárgalo de cursor.com y haz clic en "Actualizar estado" después.',
} as const

const messages = defineLocalizedCopy(ptBR, enUS, esES)

export function localAiCopyForLocale(locale: string) {
  return localizedCopyFor(messages, locale)
}

export function formatLocalAiCopy(template: string, vars: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (_match, key: string) =>
    vars[key] !== undefined ? String(vars[key]) : `{${key}}`,
  )
}
