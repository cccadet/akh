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

Dentro de um repositório já vinculado, `--project` é opcional ao criar uma
task. O Akh detecta o projeto pelo diretório Git e pelo remote:

```bash
akh task create "Implementar continuidade"
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

## M2: servidor e sincronização

O servidor usa Axum, Tokio e SQLx. Para uso individual, SQLite funciona sem
Docker nem configuração adicional:

```bash
cargo run -p akh-local-server
```

Por padrão, o banco fica em `akh.db` (ignorado pelo Git). Para escolher outro
arquivo, defina `DATABASE_URL`, por exemplo `sqlite://dados/akh.db`.

Para equipes, use PostgreSQL:

```bash
docker compose up -d

# PowerShell
$env:DATABASE_URL = "postgres://akh:akh-local-only@127.0.0.1:5432/akh"
cargo run -p akh-server
```

Na primeira conexão, registre o usuário e sincronize os dados locais:

```bash
akh register --server http://127.0.0.1:3000 --email you@example.com --username you
akh sync
```

O servidor recebe somente projects, tasks, mensagens, sessões, handoffs e
referências Git. Caminhos locais, worktrees e arquivos não commitados não são
enviados.

Quando há autenticação configurada, o fluxo `akh task ID AGENT` sincroniza
automaticamente antes de abrir e depois de fechar o agente. Tasks remotas novas
são adicionadas localmente. Um commit só é publicado como estado compartilhado
quando existe em `origin/BRANCH`; mudanças não commitadas permanecem locais.

## M3: Desktop

O Desktop usa Tauri 2, React e TypeScript:

```bash
cd apps/desktop
npm install
npm run build
npm run tauri dev
```

A interface mostra vínculos locais, tasks, estado local/compartilhado, último
agente e launchers que abrem Claude ou Codex no terminal do sistema. Novas
tasks podem ser criadas pela própria interface, escolhendo projeto, título e,
opcionalmente, branch.

Em **Settings** também é possível registrar/login, sincronizar, configurar o
terminal, escolher a raiz dos worktrees e criar profiles de agentes. Por
exemplo, a CLI equivalente para Headroom é:

```bash
akh profile set codex-headroom headroom wrap codex
akh terminal set wezterm start
akh worktree set D:\\worktrees
```

Backups portáteis podem ser exportados e importados em **Settings**. Eles
incluem projetos, tasks, conversas, perfis e o SQLite, mas nunca tokens,
worktrees ou caminhos locais. Depois de importar em outra máquina, vincule
novamente os clones locais dos projetos.

Os mesmos recursos estão disponíveis na CLI:

```bash
akh backup export akh-backup.akh-backup --database /caminho/para/akh.db
akh backup import akh-backup.akh-backup --database /caminho/para/akh.db
```

O argumento `--database` é opcional. Quando usado na importação, o SQLite é
preparado para ativação na próxima inicialização do Desktop.

O aplicativo Desktop inicia automaticamente o servidor SQLite embutido em
`127.0.0.1:3000`. O banco é salvo no diretório de dados do aplicativo e não
exige Docker nem um processo separado. O instalador Windows também inclui
`akh.exe` e adiciona sua pasta ao `PATH` do usuário; um novo terminal já pode
usar `akh --help`. Para gerar o instalador:

```powershell
cd apps/desktop
npm install
.\\node_modules\\.bin\\tauri.cmd build
```

## M4: workflow de equipe

Depois de sincronizar uma task, ela pode ser entregue a outro usuário:

```bash
akh handoff 1 --to USER_UUID --note "Implementação pronta; faltam testes no Windows."
```

O servidor registra sessões de agente, handoffs e notificações. A GUI também
oferece a ação de handoff e diferencia estado local de estado compartilhado.

## Releases

O workflow [Release](.github/workflows/release.yml) roda ao enviar uma tag
`vX.Y.Z`. A versão da tag deve corresponder às versões do workspace Rust,
Tauri e frontend. A publicação só ocorre após os builds Windows e Linux
terminarem com sucesso. A Release contém o instalador Windows com a CLI,
um pacote `.deb` para Linux e um arquivo `akh-vX.Y.Z-linux-x86_64.tar.gz`
com a CLI independente para Linux/WSL.

Para publicar a versão atual, após enviar os commits desejados:

```bash
git tag v0.1.1
git push origin v0.1.1
```

No WSL x86_64, baixe o arquivo da página de Releases, extraia `akh` e
coloque-o em um diretório do `PATH`:

```bash
tar -xzf akh-v0.1.1-linux-x86_64.tar.gz
install -Dm755 akh "$HOME/.local/bin/akh"
fish_add_path "$HOME/.local/bin" # Fish; no Bash, exporte PATH="$HOME/.local/bin:$PATH"
akh --version
```
