import { workspace } from 'vscode';

export interface ServerSettings {
  customTags: string[];
  keyOrdering: boolean;
  kubernetesVersion: string;
  schemaStore: boolean;
  formatValidation: boolean;
  formatPrintWidth?: number;
  formatSingleQuote: boolean;
  formatPreserveQuotes: boolean;
  formatBracketSpacing: boolean;
  httpProxy: string;
  colorDecorators: boolean;
  schemas: Record<string, string>;
  yamlVersion: string;
  validate: boolean;
  flowStyle: string;
  formatEnforceBlockStyle: boolean;
  duplicateKeys: string;
  formatRemoveDuplicateKeys: boolean;
  formatIndentSequences: boolean;
  formatEnable: boolean;
  formatRespectEditorconfig: boolean;
}

export function getConfig(): ServerSettings {
  const cfg = workspace.getConfiguration('rlsp-yaml');
  const printWidthInspect = cfg.inspect<number>('formatPrintWidth');
  const printWidthValue =
    printWidthInspect?.workspaceFolderValue ??
    printWidthInspect?.workspaceValue ??
    printWidthInspect?.globalValue;
  return {
    customTags: cfg.get<string[]>('customTags', []),
    keyOrdering: cfg.get('keyOrdering', false),
    kubernetesVersion: cfg.get('kubernetesVersion', 'master'),
    schemaStore: cfg.get('schemaStore', true),
    formatValidation: cfg.get('formatValidation', true),
    ...(printWidthValue !== undefined && { formatPrintWidth: printWidthValue }),
    formatSingleQuote: cfg.get('formatSingleQuote', false),
    formatPreserveQuotes: cfg.get('formatPreserveQuotes', false),
    formatBracketSpacing: cfg.get('formatBracketSpacing', true),
    httpProxy: cfg.get('httpProxy', ''),
    colorDecorators: cfg.get('colorDecorators', true),
    schemas: cfg.get<Record<string, string>>('schemas', {}),
    yamlVersion: cfg.get<string>('yamlVersion', '1.2'),
    validate: cfg.get<boolean>('validate', true),
    flowStyle: cfg.get<string>('flowStyle', 'warning'),
    formatEnforceBlockStyle: cfg.get<boolean>('formatEnforceBlockStyle', false),
    duplicateKeys: cfg.get<string>('duplicateKeys', 'error'),
    formatRemoveDuplicateKeys: cfg.get<boolean>('formatRemoveDuplicateKeys', false),
    formatIndentSequences: cfg.get<boolean>('formatIndentSequences', true),
    formatEnable: cfg.get<boolean>('formatEnable', true),
    formatRespectEditorconfig: cfg.get<boolean>('formatRespectEditorconfig', true),
  };
}
