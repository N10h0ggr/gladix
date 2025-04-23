import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "@/components/ui/table"

// Define telemetry row shape
export interface DataRow {
  sensorId: string  // Unique identifier for the EDR sensor
  timestamp: string // ISO timestamp of the reading
  cpuUsage: number  // Percentage CPU usage
  memoryUsage: number // Memory usage in MB
  processCount: number // Number of active processes
  networkIn: number  // Incoming network KB
  networkOut: number // Outgoing network KB
}

// Dummy data example: how to normalize readings from sensors
export const telemetryDummyData: DataRow[] = [
  {
    sensorId: "EDR-001",
    timestamp: "2025-04-19T10:15:30Z",
    cpuUsage: 12.5,
    memoryUsage: 256,
    processCount: 45,
    networkIn: 1024,
    networkOut: 2048,
  },
  {
    sensorId: "EDR-002",
    timestamp: "2025-04-19T10:16:00Z",
    cpuUsage: 7.8,
    memoryUsage: 128,
    processCount: 30,
    networkIn: 512,
    networkOut: 1024,
  },
  {
    sensorId: "EDR-003",
    timestamp: "2025-04-19T10:16:30Z",
    cpuUsage: 25.3,
    memoryUsage: 512,
    processCount: 60,
    networkIn: 2048,
    networkOut: 4096,
  },
]

// Telemetry table without drag or sorting
export function DataTable({ data }: { data: DataRow[] }) {
  return (
    <div className="overflow-auto">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Sensor ID</TableHead>
            <TableHead>Timestamp</TableHead>
            <TableHead>CPU (%)</TableHead>
            <TableHead>Memory (MB)</TableHead>
            <TableHead>Processes</TableHead>
            <TableHead>Net In (KB)</TableHead>
            <TableHead>Net Out (KB)</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {data.map((row) => (
            <TableRow key={`${row.sensorId}-${row.timestamp}`}>                
              <TableCell>{row.sensorId}</TableCell>
              <TableCell>{new Date(row.timestamp).toLocaleString()}</TableCell>
              <TableCell>{row.cpuUsage.toFixed(1)}</TableCell>
              <TableCell>{row.memoryUsage}</TableCell>
              <TableCell>{row.processCount}</TableCell>
              <TableCell>{row.networkIn}</TableCell>
              <TableCell>{row.networkOut}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}
