# Build stage
FROM rust:alpine3.17 AS build
WORKDIR /usr/src/droxporter
COPY . .
RUN apk add --no-cache build-base
RUN cargo test --release
RUN cargo build --release

# Final image stage
FROM alpine:3.17
WORKDIR /app
COPY --from=build /usr/src/dropxporter/target/release/droxporter /app/dropxporter
CMD ["/app/droxporter"]