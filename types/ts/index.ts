export type DeviceType = "solarInverter" | "batterySystem" | "chargeController" | "energyMeter"

export type HealthStatus = "healthy" | "warning" | "critical" | "offline"

export type DeviceStatus = { isConnected: boolean; lastSeen: string; health: HealthStatus; errorMessage: string | null }

export type DeviceMetrics = { inputPowerWatts: number | null; outputPowerWatts: number | null; loadPercentage: number | null; batteryVoltage: number | null; batteryCurrent: number | null; batterySocPercentage: number | null; batteryTemperatureCelsius: number | null; pvVoltage: number | null; pvCurrent: number | null; pvPowerWatts: number | null; gridVoltage: number | null; gridFrequency: number | null; gridPowerWatts: number | null; deviceTemperatureCelsius: number | null; efficiencyPercentage: number | null; faultCodes: string[]; operatingMode: string | null; customMetrics: { [key: string]: number } }

export type DeviceData = { deviceId: string; timestamp: string; deviceType: DeviceType; metrics: DeviceMetrics; status: DeviceStatus; rawData: string | null }

export type DeviceConfigDto = { id: string; name: string; deviceType: DeviceType; protocolName: string; enabled: boolean; pollIntervalSeconds: number; connectionParams: { [key: string]: string } }

export type AddDeviceRequestDto = { id: string; name: string; deviceType: DeviceType; protocolName: string; enabled: boolean; pollIntervalSeconds: number; connectionParams: { [key: string]: string } }

export type DeviceListItemDto = { id: string; name: string; deviceType: DeviceType; protocolName: string; enabled: boolean; pollIntervalSeconds: number; connectionParams: { [key: string]: string }; isPolling: boolean }

export type TestConnectionParamsDto = { deviceType: DeviceType; protocolName: string; connectionParams: { [key: string]: string } }

export type TestConnectionResponseDto = { ok: boolean; message: string | null }

export type ResourceUsageDto = { current: number; peak: number; average: number; unit: string }

export type StorageUsageDto = { usedMb: number; totalMb: number; percent: number }

export type SystemStatusDto = { uptimeSeconds: number; version: string; activeDevices: number; activeConnections: number; activeClients: number; dataPointsPerSecond: number; memoryUsage: ResourceUsageDto; cpuUsage: ResourceUsageDto; storageUsage: StorageUsageDto }

export type ErrorResponseDto = { error: string; details: string; timestamp: string }

export type Eg4Command = { type: "writeRegister"; addr: number; value: number } | { type: "writeRegisters"; addr: number; values: number[] } | { type: "writeCoil"; addr: number; value: boolean } | { type: "writeCoils"; addr: number; values: boolean[] } | { type: "setMaxChargeCurrent"; amps: number }

export type DeviceCommandRequest = { protocol: "eg4-6000xp-modbus"; command: Eg4Command }

export type DeviceCommandResponseDto = { ok: boolean; message: string | null }

export type SettingValueDto = { kind: "number"; value: number; min: number; max: number; step: number; unit: string | null } | { kind: "toggle"; enabled: boolean } | { kind: "choice"; value: number; options: number[]; unit: string | null } | { kind: "timeWindow"; start: string; end: string }

export type DeviceSettingDto = { key: string; label: string; group: string; setting: SettingValueDto }

export type WriteSettingRequestDto = { value: string }