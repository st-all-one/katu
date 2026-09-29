//! Especificações declarativas das 11 tools do registry (E06-T01/E06-T02).
//!
//! Cada entrada reflete [`katu_policy::ToolArgs`]: são a fonte que o linter (`super::lint_all`)
//! valida no CI. Manter em sincronia com o registry e com os executores.

use super::{ParamKind, ParamSpec, ToolSchema};

/// Especificação de leitura.
const READ: ToolSchema<'static> = ToolSchema {
    name: "read",
    description: "Use when you need file contents or structure. Do not use for searching many files (use grep or find).",
    params: &[
        ParamSpec {
            name: "path",
            kind: ParamKind::Path,
            required: true,
            description: "caminho do ficheiro a ler",
        },
        ParamSpec {
            name: "view",
            kind: ParamKind::Enum(&["full", "range", "outline", "summary", "symbol", "diff"]),
            required: false,
            description: "forma do resultado (por omissão `summary`)",
        },
        ParamSpec {
            name: "range",
            kind: ParamKind::Text,
            required: false,
            description: "intervalo de linhas `inicio:fim` (view `range`)",
        },
        ParamSpec {
            name: "symbol",
            kind: ParamKind::Text,
            required: false,
            description: "nome do símbolo (view `symbol`)",
        },
        ParamSpec {
            name: "base",
            kind: ParamKind::Text,
            required: false,
            description: "conteúdo da versão anterior (view `diff`)",
        },
    ],
};

/// Especificação de escrita.
const WRITE: ToolSchema<'static> = ToolSchema {
    name: "write",
    description: "Use when creating a brand-new file. Do not use for changing an existing file (use edit).",
    params: &[
        ParamSpec {
            name: "path",
            kind: ParamKind::Path,
            required: true,
            description: "caminho do ficheiro novo",
        },
        ParamSpec {
            name: "content",
            kind: ParamKind::Text,
            required: true,
            description: "conteúdo integral do ficheiro",
        },
    ],
};

/// Especificação de edição.
const EDIT: ToolSchema<'static> = ToolSchema {
    name: "edit",
    description: "Use when changing an existing file with a unique anchor. Do not use for new files (use write).",
    params: &[
        ParamSpec {
            name: "path",
            kind: ParamKind::Path,
            required: true,
            description: "caminho do ficheiro a editar",
        },
        ParamSpec {
            name: "old",
            kind: ParamKind::Text,
            required: true,
            description: "trecho único a substituir (tem de aparecer exatamente uma vez)",
        },
        ParamSpec {
            name: "new",
            kind: ParamKind::Text,
            required: true,
            description: "trecho que substitui `old`",
        },
        ParamSpec {
            name: "dry_run",
            kind: ParamKind::Boolean,
            required: false,
            description: "mostra o patch sem gravar",
        },
    ],
};

/// Especificação de movimento.
const MOVE: ToolSchema<'static> = ToolSchema {
    name: "move",
    description: "Use when renaming or relocating a file. Do not use for copying contents between files.",
    params: &[
        ParamSpec {
            name: "from",
            kind: ParamKind::Path,
            required: true,
            description: "caminho de origem",
        },
        ParamSpec {
            name: "to",
            kind: ParamKind::Path,
            required: true,
            description: "caminho de destino (não pode existir)",
        },
    ],
};

/// Especificação da lixeira.
const TRASH: ToolSchema<'static> = ToolSchema {
    name: "trash",
    description: "Use when removing a file recoverably. Do not use for permanent deletion.",
    params: &[ParamSpec {
        name: "path",
        kind: ParamKind::Path,
        required: true,
        description: "caminho a enviar para a lixeira do projeto",
    }],
};

/// Especificação de execução.
const BASH: ToolSchema<'static> = ToolSchema {
    name: "bash",
    description: "Use when running a program with known argv. Do not use for evaluating a shell string.",
    params: &[
        ParamSpec {
            name: "argv",
            kind: ParamKind::ListText,
            required: true,
            description: "programa e argumentos, já resolvidos (sem shell)",
        },
        ParamSpec {
            name: "cwd",
            kind: ParamKind::Path,
            required: false,
            description: "diretório de trabalho (por omissão a raiz do workspace)",
        },
    ],
};

/// Especificação de busca por conteúdo.
const GREP: ToolSchema<'static> = ToolSchema {
    name: "grep",
    description: "Use when searching the contents of many files. Do not use for filenames (use find).",
    params: &[
        ParamSpec {
            name: "query",
            kind: ParamKind::Text,
            required: true,
            description: "texto a procurar no conteúdo",
        },
        ParamSpec {
            name: "root",
            kind: ParamKind::Path,
            required: false,
            description: "raiz da busca (por omissão o workspace)",
        },
        ParamSpec {
            name: "limit",
            kind: ParamKind::Integer,
            required: false,
            description: "número máximo de resultados",
        },
    ],
};

/// Especificação de busca por nome.
const FIND: ToolSchema<'static> = ToolSchema {
    name: "find",
    description: "Use when searching for filenames. Do not use for file contents (use grep).",
    params: &[
        ParamSpec {
            name: "query",
            kind: ParamKind::Text,
            required: true,
            description: "texto a procurar no nome do ficheiro",
        },
        ParamSpec {
            name: "root",
            kind: ParamKind::Path,
            required: false,
            description: "raiz da busca (por omissão o workspace)",
        },
    ],
};

/// Especificação de listagem.
const LS: ToolSchema<'static> = ToolSchema {
    name: "ls",
    description: "Use when mapping a directory's languages and exports. Do not use for reading file contents (use read).",
    params: &[ParamSpec {
        name: "path",
        kind: ParamKind::Path,
        required: true,
        description: "diretório a mapear",
    }],
};

/// Especificação de planeamento.
const PLAN: ToolSchema<'static> = ToolSchema {
    name: "plan",
    description: "Use when recording the task plan artifact. Do not use for long-term memory (use memory).",
    params: &[
        ParamSpec {
            name: "goal",
            kind: ParamKind::Text,
            required: true,
            description: "objetivo da tarefa",
        },
        ParamSpec {
            name: "next_action",
            kind: ParamKind::Text,
            required: true,
            description: "próxima ação concreta",
        },
    ],
};

/// Especificação de memória explícita.
const MEMORY: ToolSchema<'static> = ToolSchema {
    name: "memory",
    description: "Use when explicitly recording or recalling long-term memory. Do not use for ordinary file edits.",
    params: &[
        ParamSpec {
            name: "command",
            kind: ParamKind::Enum(&["record", "search"]),
            required: true,
            description: "operação de memória",
        },
        ParamSpec {
            name: "statement",
            kind: ParamKind::Text,
            required: false,
            description: "facto a gravar (comando `record`)",
        },
        ParamSpec {
            name: "anchor",
            kind: ParamKind::Id("^[a-z0-9][a-z0-9:_-]{0,63}$"),
            required: false,
            description: "âncora que liga a nota ao artefacto (comando `record`)",
        },
    ],
};

/// As 11 especificações, na mesma ordem canónica do registry.
pub const SCHEMAS: &[ToolSchema<'static>] = &[
    READ, WRITE, EDIT, MOVE, TRASH, BASH, GREP, FIND, LS, PLAN, MEMORY,
];
