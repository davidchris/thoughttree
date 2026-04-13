type LogMethod = (...args: unknown[]) => void;

const isDev = import.meta.env.DEV;

const noop: LogMethod = () => {};

function bindConsole(method: keyof Console): LogMethod {
  return (...args: unknown[]) => {
    (console[method] as (...inner: unknown[]) => void)(...args);
  };
}

export const logger: Record<'debug' | 'info' | 'warn' | 'error', LogMethod> = {
  debug: isDev ? bindConsole('debug') : noop,
  info: isDev ? bindConsole('info') : noop,
  warn: bindConsole('warn'),
  error: bindConsole('error'),
};
