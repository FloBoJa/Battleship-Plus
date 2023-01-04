FROM battleship-server-rs

RUN --mount=type=cache,target=/var/cache/apt \
        apt-get update \
        && DEBIAN_FRONTEND=noninteractive \
            apt-get install --no-install-recommends --assume-yes \
              tcpdump

RUN mkdir /data
WORKDIR /battleship_plus
COPY docker/logger_entrypoint.sh logger_entrypoint.sh
RUN chmod +x logger_entrypoint.sh

CMD /battleship_plus/logger_entrypoint.sh
