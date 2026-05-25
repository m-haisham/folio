install:
    cargo install --locked --path crates/folio-cli --bin folio --force

deploy VERSION:
    git tag {{ VERSION }}
    git push
    git push --tags

up:
    docker compose up -d

down:
    docker compose down
