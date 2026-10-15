/** A reference: its text. */
export type LinoRef = string;
/** A link: an optional id and its values. */
export interface LinoLink {
  id: string | null;
  values: LinoNode[];
}
export type LinoNode = LinoRef | LinoLink;
export type LinoDocument = LinoNode[];

export function group(values: LinoNode[]): LinoLink;
export function named(id: string, values: LinoNode[]): LinoLink;
export function encodeReferenceLiteral(text: string): string;
export function decodeReferenceLiteral(literal: string): string;
export function formatReference(text: string): string;
export function formatLink(link: LinoNode): string;
export function formatDocument(document: LinoDocument): string;
export function parseDocument(text: string): LinoDocument;
