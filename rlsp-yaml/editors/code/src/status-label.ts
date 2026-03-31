export type ServerState = 'starting' | 'running' | 'stopped' | 'error';

export interface StatusLabel {
  text: string;
  tooltip: string;
}

export function statusBarLabel(state: ServerState): StatusLabel {
  switch (state) {
    case 'starting':
      return { text: '$(sync~spin) rlsp-yaml', tooltip: 'rlsp-yaml: starting' };
    case 'running':
      return { text: '$(check) rlsp-yaml', tooltip: 'rlsp-yaml: running' };
    case 'stopped':
      return { text: '$(x) rlsp-yaml', tooltip: 'rlsp-yaml: stopped' };
    case 'error':
      return { text: '$(warning) rlsp-yaml', tooltip: 'rlsp-yaml: error' };
  }
}
