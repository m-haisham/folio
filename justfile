install:
    cargo install --locked --path crates/folio-cli --bin folio

deploy VERSION:
    git tag {{ VERSION }}
    git push
    git push --tags
