import { inject, shallowRef, type InjectionKey, type Ref } from 'vue'

export interface SelectedGameContext {
  appId: string
  gameId: string
  gameName: string
  engine: string | null
}

export const selectedGameKey: InjectionKey<Readonly<Ref<SelectedGameContext | null>>> = Symbol('selectedGame')

/** Live library selection; an absent provider means no selected game. */
export function useSelectedGame() {
  return inject(selectedGameKey, shallowRef<SelectedGameContext | null>(null))
}
