import { defineLocalizedCopy, localizedCopyFor } from '../../i18n/localizedCopy'

const globalToolsMessages = defineLocalizedCopy(
  { organize: 'Mods e perfis', system: 'Sistema', create: 'Ajuda e criação' },
  { organize: 'Mods & profiles', system: 'System', create: 'Help & create' },
  { organize: 'Mods y perfiles', system: 'Sistema', create: 'Ayuda y creación' },
)

export function globalToolsCopyForLocale(locale: string) {
  return localizedCopyFor(globalToolsMessages, locale)
}
