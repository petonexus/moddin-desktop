import { defineLocalizedCopy, localizedCopyFor } from '../../i18n/localizedCopy'

/**
 * Copy for the app self-updater.
 *
 * Every sentence here is about the one thing a user cannot check for
 * themselves — whether the thing they are about to install is really from
 * us — so the wording says what was verified and what a failure leaves
 * behind, rather than what the updater was doing.
 */
const ptBR = {
  button: 'Atualizar o Moddin',
  title: 'Atualizar o Moddin',
  subtitle: 'O Moddin só instala versões estáveis assinadas pela chave de lançamento dele.',
  close: 'Fechar',
  check: 'Procurar atualizações',
  checking: 'Procurando…',
  channelLabel: 'Canal',
  installedVersion: 'Sua versão',
  upToDateTitle: 'Você está na última versão',
  upToDateDescription: 'Nada novo para instalar agora.',
  notConfiguredTitle: 'Esta build não tem chave de assinatura',
  notConfiguredDescription:
    'A chave pública de atualização ainda não foi colocada nesta build, então o Moddin não consegue verificar um instalador. Nada foi baixado. Você pode continuar usando esta versão normalmente.',
  unavailableTitle: 'Esta versão não é oferecida',
  install: 'Instalar e reiniciar',
  installCancel: 'Agora não',
  confirmTitle: 'Instalar a versão {version}?',
  confirmDescription: 'O Moddin vai baixar {version}, conferir a assinatura e reiniciar.',
  confirmDetailDownload:
    'O instalador só é executado depois que a assinatura dele bate com a chave de lançamento do Moddin. Sem essa conferida, nada é instalado.',
  confirmDetailRelaunch:
    'No Windows o Moddin fecha e abre de novo sozinho quando a instalação termina. Se algo der errado, a versão que você tem agora continua funcionando.',
  downloading: 'Baixando {percent}',
  verifying: 'Conferindo a assinatura…',
  finishing: 'Instalando. O Moddin vai fechar e abrir de novo…',
  released: 'Publicado em {date}',
  declinedNotice: 'Você recusou a versão {version}. Ela não vai ser oferecida de novo.',
  declinedAgain: 'Mostrar de novo',
  errorCheckTitle: 'Não consegui procurar atualizações.',
  errorCheckWhy:
    'O Moddin não conseguiu falar com o GitHub agora. Isso não muda nada na sua instalação — pode tentar de novo daqui a pouco.',
  errorInstallTitle: 'A atualização não foi instalada.',
  errorInstallWhy:
    'Nada foi trocado no seu Moddin. A versão que você está usando continua instalada e funciona igual.',
} as const

const enUS = {
  button: 'Update Moddin',
  title: 'Update Moddin',
  subtitle: 'Moddin only installs stable releases signed with its own release key.',
  close: 'Close',
  check: 'Check for updates',
  checking: 'Checking…',
  channelLabel: 'Channel',
  installedVersion: 'Your version',
  upToDateTitle: 'You are up to date',
  upToDateDescription: 'There is nothing new to install right now.',
  notConfiguredTitle: 'This build has no release key',
  notConfiguredDescription:
    'The update public key has not been added to this build yet, so Moddin cannot verify an installer. Nothing was downloaded. You can keep using this version as it is.',
  unavailableTitle: 'That release is not offered here',
  install: 'Install and restart',
  installCancel: 'Not now',
  confirmTitle: 'Install version {version}?',
  confirmDescription: 'Moddin will download {version}, check its signature, and restart.',
  confirmDetailDownload:
    'The installer only runs after its signature matches the Moddin release key. Without that check, nothing is installed.',
  confirmDetailRelaunch:
    'On Windows Moddin closes and opens again by itself when the install finishes. If anything goes wrong, the version you have now keeps working.',
  downloading: 'Downloading {percent}',
  verifying: 'Checking the signature…',
  finishing: 'Installing. Moddin will close and open again…',
  released: 'Released {date}',
  declinedNotice: 'You turned down version {version}. It will not be offered again.',
  declinedAgain: 'Show it again',
  errorCheckTitle: 'Could not check for updates.',
  errorCheckWhy:
    'Moddin could not reach GitHub just now. Nothing about your installation changed — try again in a little while.',
  errorInstallTitle: 'The update was not installed.',
  errorInstallWhy:
    'Nothing on your Moddin was replaced. The version you are running is still installed and works the same.',
} as const

const esES = {
  button: 'Actualizar Moddin',
  title: 'Actualizar Moddin',
  subtitle: 'Moddin solo instala versiones estables firmadas con su propia clave de lanzamiento.',
  close: 'Cerrar',
  check: 'Buscar actualizaciones',
  checking: 'Buscando…',
  channelLabel: 'Canal',
  installedVersion: 'Tu versión',
  upToDateTitle: 'Estás al día',
  upToDateDescription: 'Ahora mismo no hay nada nuevo que instalar.',
  notConfiguredTitle: 'Esta build no tiene clave de firma',
  notConfiguredDescription:
    'La clave pública de actualización todavía no está en esta build, así que Moddin no puede verificar un instalador. No se descargó nada. Puedes seguir usando esta versión tal cual.',
  unavailableTitle: 'Esa versión no se ofrece aquí',
  install: 'Instalar y reiniciar',
  installCancel: 'Ahora no',
  confirmTitle: '¿Instalar la versión {version}?',
  confirmDescription: 'Moddin descargará {version}, comprobará su firma y se reiniciará.',
  confirmDetailDownload:
    'El instalador solo se ejecuta cuando su firma coincide con la clave de lanzamiento de Moddin. Sin esa comprobación no se instala nada.',
  confirmDetailRelaunch:
    'En Windows Moddin se cierra y se abre solo cuando termina la instalación. Si algo falla, la versión que ya tienes sigue funcionando.',
  downloading: 'Descargando {percent}',
  verifying: 'Comprobando la firma…',
  finishing: 'Instalando. Moddin se cerrará y se abrirá de nuevo…',
  released: 'Publicado el {date}',
  declinedNotice: 'Rechazaste la versión {version}. No se volverá a ofrecer.',
  declinedAgain: 'Volver a mostrarla',
  errorCheckTitle: 'No pude buscar actualizaciones.',
  errorCheckWhy:
    'Moddin no pudo llegar a GitHub en este momento. Tu instalación no ha cambiado: inténtalo otra vez en un rato.',
  errorInstallTitle: 'La actualización no se instaló.',
  errorInstallWhy:
    'No se reemplazó nada en tu Moddin. La versión que estás usando sigue instalada y funciona igual.',
} as const

const messages = defineLocalizedCopy(ptBR, enUS, esES)

export function appUpdateCopyForLocale(locale: string) {
  return localizedCopyFor(messages, locale)
}

export function formatAppUpdateCopy(
  template: string,
  vars: Record<string, string | number>,
): string {
  return template.replace(/\{(\w+)\}/g, (_match, key: string) =>
    vars[key] !== undefined ? String(vars[key]) : `{${key}}`,
  )
}
