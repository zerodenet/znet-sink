export interface ProxyNode {
  id: string;
  name: string;
  protocol: string;
  delay: number;
  domain: string;
}

export interface CoreStatus {
  running: boolean;
  pid: number;
  uploadSpeed: string;
  downloadSpeed: string;
}
