import { createContext, useContext } from 'react'

export type ExplorerTarget =
  | { page: 'transaction'; id: string }
  | { page: 'address'; id: string }
  | { page: 'token'; id: string }
  | { page: 'block'; id: string }

type ExplorerNavContextType = {
  /** Navigate to the built-in explorer for a given entity. */
  navigateToExplorer: (target: ExplorerTarget) => void
}

const ExplorerNavContext = createContext<ExplorerNavContextType>({
  navigateToExplorer: () => {},
})

export const ExplorerNavProvider = ExplorerNavContext.Provider

export function useExplorerNav() {
  return useContext(ExplorerNavContext)
}
