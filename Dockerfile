FROM rust:alpine3.22 AS build
WORKDIR /usr/src/droxporter
COPY . .
RUN apk add --no-cache build-base
RUN rustup update stable
RUN cargo test --release
RUN cargo build --release


# Final image stage
FROM alpine:3.17
WORKDIR /app
COPY --from=build /usr/src/droxporter/target/release/droxporter /app/droxporter
COPY config.yml /app/config.yml
CMD ["/app/droxporter"]