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