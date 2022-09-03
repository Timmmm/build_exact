export type BuildEnvironment =  {
  [key: string]: string;
};

export interface BuildCommand {
  command: string[];
  inputs: string[];
  outputs: string[];
  workingDir: string;
  env: BuildEnvironment;
}

export interface TestCommand {
  command: string[];
  inputs: string[];
  workingDir: string;
  env: BuildEnvironment;
}

export type TestSet = {
  [key: string]: TestCommand;
};

export interface BuildDescription {
  commands: BuildCommand[];
  tests: TestSet;
  sandboxedDirs: string[];
}

export function exportBuild(desc: BuildDescription) {
  console.log(JSON.stringify(desc));
}
