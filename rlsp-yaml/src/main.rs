// SPDX-License-Identifier: MIT

use rlsp_yaml::server::Backend;
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
    let (service, socket) = LspService::new(Backend::new);
    let stdin = stdin();
    let stdout = stdout();
    Server::new(stdin, stdout, socket).serve(service).await;
}
