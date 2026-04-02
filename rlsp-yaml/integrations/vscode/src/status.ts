import { StatusBarAlignment, window, type Disposable, type StatusBarItem } from 'vscode';
import { statusBarLabel, type ServerState } from './status-label.js';

export type { ServerState } from './status-label.js';
export { statusBarLabel } from './status-label.js';

export class StatusBar implements Disposable {
  private readonly item: StatusBarItem;

  constructor() {
    this.item = window.createStatusBarItem(StatusBarAlignment.Left, 0);
    this.item.command = 'rlsp-yaml.showOutput';
    this.update('stopped');
    this.item.show();
  }

  update(state: ServerState): void {
    const label = statusBarLabel(state);
    this.item.text = label.text;
    this.item.tooltip = label.tooltip;
  }

  dispose(): void {
    this.item.dispose();
  }
}
