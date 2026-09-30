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
  errorPermissionTitle: 'O Windows bloqueou a alteração.',
  errorPermissionWhy: 'O arquivo de configuração dessa IA fica numa pasta protegida. Rode o Moddin como administrador e tente de novo.',
  errorResourceTitle: 'A pasta do servidor MCP não foi encontrada.',
  errorResourceWhy: 'O Moddin procura um servidor MCP na sua pasta de usuário. Clique em "Atualizar status" para o Moddin criar de novo.',
  errorDetectTitle: 'Não consegui procurar as IAs instaladas.',
  errorDetectWhy: 'Algo deu errado ao varrer o PC. Tente de novo em alguns segundos.',
  errorWriteTitle: 'Não consegui alterar a configuração da IA.',
  errorWriteWhy: 'O arquivo de configuração dessa IA não pôde ser reescrito — ele pode estar aberto no editor ou ter sido alterado por outra ferramenta. Nada foi mudado; tente de novo.',
  disconnectConfirmTitle: 'Desconectar este agente?',
  disconnectConfirmDescription: '{name} vai parar de responder ao Moddin.',
  disconnectConfirmDetail: 'O Moddin reescreve o arquivo de configuração do seu editor para apontar para o servidor do Moddin — desfazer devolve o conteúdo original.',
  disconnectCancel: 'Cancelar',
  // UX-26: one Connect and one Disconnect per agent, so the button name
  // carries the agent. `{action}` stays the visible text, which keeps
  // the two in step for voice control.
  actionNamed: '{action} {name}',
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
  errorPermissionTitle: 'Windows blocked the change.',
  errorPermissionWhy: "This AI's config file sits in a protected folder. Run Moddin as administrator and try again.",
  errorResourceTitle: 'The MCP server folder was not found.',
  errorResourceWhy: 'Moddin looks for an MCP server in your user folder. Click "Refresh status" and Moddin will create it again.',
  errorDetectTitle: 'Could not look for installed AIs.',
  errorDetectWhy: 'Something went wrong while scanning the PC. Try again in a few seconds.',
  errorWriteTitle: "Could not update this AI's configuration.",
  errorWriteWhy: "That AI's config file could not be rewritten — it may be open in the editor, or another tool changed it. Nothing was changed; try again.",
  disconnectConfirmTitle: 'Disconnect this agent?',
  disconnectConfirmDescription: '{name} will stop responding to Moddin.',
  disconnectConfirmDetail: 'Moddin rewrites your editor config to point at the Moddin server — undoing restores the original content.',
  disconnectCancel: 'Cancel',
  actionNamed: '{action} {name}',
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
  errorPermissionTitle: 'Windows bloqueó el cambio.',
  errorPermissionWhy: 'El archivo de configuración de esa IA está en una carpeta protegida. Ejecuta Moddin como administrador e inténtalo otra vez.',
  errorResourceTitle: 'No se encontró la carpeta del servidor MCP.',
  errorResourceWhy: 'Moddin busca un servidor MCP en tu carpeta de usuario. Haz clic en "Actualizar estado" y Moddin la creará de nuevo.',
  errorDetectTitle: 'No pude buscar las IAs instaladas.',
  errorDetectWhy: 'Algo salió mal al revisar el PC. Inténtalo otra vez en unos segundos.',
  errorWriteTitle: 'No pude cambiar la configuración de la IA.',
  errorWriteWhy: 'No se pudo reescribir el archivo de configuración de esa IA: puede estar abierto en el editor, u otra herramienta lo cambió. No se cambió nada; inténtalo otra vez.',
  disconnectConfirmTitle: '¿Desconectar este agente?',
  disconnectConfirmDescription: '{name} dejará de responder a Moddin.',
  disconnectConfirmDetail: 'Moddin reescribe la configuración de tu editor para apuntar al servidor de Moddin — deshacer devuelve el contenido original.',
  disconnectCancel: 'Cancelar',
  actionNamed: '{action} {name}',
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
