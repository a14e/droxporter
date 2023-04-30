# Build stage
FROM rust:alpine AS build
WORKDIR /usr/src/dropxporter
COPY . .
RUN apk add --no-cache build-base
RUN cargo test --release
RUN cargo build --release

# Final image stage
FROM alpine:latest
WORKDIR /app
COPY --from=build /usr/src/dropxporter/target/release/dropxporter /app/dropxporter
CMD ["/app/dropxporter"]