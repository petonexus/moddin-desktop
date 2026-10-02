import { defineLocalizedCopy, localizedCopyFor } from '../../i18n/localizedCopy'

const capabilitySectionMessages = defineLocalizedCopy(
  {
    retry: 'Tentar novamente',
    loadErrorTitle: 'Não foi possível carregar os mods salvos',
    loadErrorHint: 'Tente novamente para atualizar a lista de mods disponíveis para este jogo.',
    mutationBusyHint: 'Aguarde a ação em andamento terminar antes de alterar outro mod.',
  },
  {
    retry: 'Try again',
    loadErrorTitle: 'Could not load saved mods',
    loadErrorHint: 'Try again to refresh the mods available for this game.',
    mutationBusyHint: 'Wait for the current action to finish before changing another mod.',
  },
  {
    retry: 'Volver a intentar',
    loadErrorTitle: 'No se pudieron cargar los mods guardados',
    loadErrorHint: 'Vuelve a intentar para actualizar los mods disponibles para este juego.',
    mutationBusyHint: 'Espera a que termine la acción actual antes de cambiar otro mod.',
  },
)

export function capabilitySectionCopyForLocale(locale: string) {
  return localizedCopyFor(capabilitySectionMessages, locale)
}
