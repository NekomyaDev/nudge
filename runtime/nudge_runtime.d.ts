export interface Span {
  start: number;
  end: number;
}

export interface AstNode {
  kind: string;
  span: Span;
}

export interface Expr extends AstNode {
  kind: string;
  span: Span;
  value?: any;
  name?: string;
  args?: Expr[];
  kwargs?: Record<string, Expr>;
}

export interface EffectDecl extends AstNode {
  kind: "Effect";
  name: string;
  operations: Array<{
    name: string;
    params: Array<{ name: string; type: string }>;
    returnType: string;
  }>;
}

export interface HandlerDecl extends AstNode {
  kind: "Handler";
  effect: string;
  handlers: Record<string, (...args: any[]) => any>;
}

export interface RunnerOptions {
  timeoutMs?: number;
  signal?: AbortSignal;
  model?: string;
  budget?: number;
  deadline?: number;
}

export class TimeoutError extends Error {
  constructor(message?: string);
}

export function withTimeout<T>(promise: Promise<T>, timeoutMs: number, message?: string): Promise<T>;
export function str_pad_start(s: string, width: number, pad?: string): string;
export function str_pad_end(s: string, width: number, pad?: string): string;
export function math_clamp(v: number, minV: number, maxV: number): number;
export function math_round_to(v: number, decimals?: number): number;
export function json_pretty(v: any, indent?: number): string;
export function url_encode(s: string): string;
export function url_decode(s: string): string;
export function lists_chunk<T>(xs: T[], size: number): T[][];
export function lists_flatten<T>(xs: any[]): T[];
