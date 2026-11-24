export type EnvStatus = "ok" | "warn" | "error";

export interface EnvItem {
  id: string;
  label: string;
  status: EnvStatus;
  value: string;
  path: string | null;
  hint: string | null;
}

export interface EnvReport {
  platform: string;
  major: number | null;
  items: EnvItem[];
  sdkPlatforms: string[];
  sdkBuildTools: string[];
}
