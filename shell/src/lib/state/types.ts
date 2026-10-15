// The core's data types, as the UI may use them. The UI imports types from here, never from the
// API layer directly (docs/development.md, shell layering).
export type { ConnectorInfo } from '../api/generated/ConnectorInfo';
export type { ErrorBody } from '../api/generated/ErrorBody';
export type { LaserScanReading } from '../api/generated/LaserScanReading';
export type { LogEntry } from '../api/generated/LogEntry';
export type { LogLevel } from '../api/generated/LogLevel';
export type { NewPart } from '../api/generated/NewPart';
export type { PartInfo } from '../api/generated/PartInfo';
export type { PartState } from '../api/generated/PartState';
export type { PartStateKind } from '../api/generated/PartStateKind';
export type { PortInfo } from '../api/generated/PortInfo';
export type { Reading } from '../api/generated/Reading';
export type { SignalInfo } from '../api/generated/SignalInfo';
export type { SystemInfo } from '../api/generated/SystemInfo';
export type { ConnectionStatus } from '../api/socket';
export type { WireFormat } from '../api/wire';
