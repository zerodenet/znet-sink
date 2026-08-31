type RuleSetListener = () => void;

class RuleSetSignal {
  revision = $state(0);

  private listeners = new Set<RuleSetListener>();

  markChanged(): void {
    this.revision += 1;
    for (const listener of this.listeners) listener();
  }

  onChanged(listener: RuleSetListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
}

export const ruleSetSignal = new RuleSetSignal();
