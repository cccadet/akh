# Akh

Shared task continuity for developers and coding agents.

## M0: core local

O primeiro incremento já permite detectar e vincular um repositório Git,
associar uma task local a uma branch, preparar um worktree e iniciar Claude Code
ou Codex no terminal atual.

### Desenvolvimento

```bash
cargo build --workspace
cargo test --workspace
```

### Fluxo local

```bash
# Detecta o repositório atual
cargo run -p akh-cli -- project detect

# Salva o vínculo somente em ~/.akh/config.toml
cargo run -p akh-cli -- project link --name akh

# Relaciona uma task à branch que será usada no worktree
cargo run -p akh-cli -- task-link 183 --project akh --branch task/183

# Apenas prepara o worktree
cargo run -p akh-cli -- task 183 codex --prepare-only

# Prepara o worktree e abre o agente no terminal atual
cargo run -p akh-cli -- task 183 codex
cargo run -p akh-cli -- task 183 claude
```

Por padrão, configuração e worktrees ficam abaixo de `~/.akh` e não entram no
repositório. Para testes isolados, `AKH_CONFIG_HOME` pode apontar para outro
diretório.

Veja [PLAN.md](PLAN.md) para a visão completa do produto.

## M1: sessão compartilhada local

Tasks e conversas também podem ser criadas e consultadas localmente:

```bash
# Cria a task e sua branch local planejada
cargo run -p akh-cli -- task create "Implementar continuidade" --project akh

# Lista e inspeciona tasks
cargo run -p akh-cli -- task list
cargo run -p akh-cli -- task show 1

# Abre uma sessão capturada
cargo run -p akh-cli -- task 1 codex
```

Ao abrir uma sessão, Akh injeta projeto, título, branch, commit atual e histórico
compartilhado. Um PTY transparente preserva a interface original do agente no
terminal do sistema enquanto captura input e output. As conversas permanecem
somente em `~/.akh/conversations` nesta etapa; sincronização entre máquinas faz
parte do M2.
