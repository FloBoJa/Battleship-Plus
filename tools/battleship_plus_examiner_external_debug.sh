nohup /bin/terminator --geometry 1200x700 -e "gdbserver --once 127.0.0.1:5555 target/debug/battleship_plus_examiner interactive" &
sleep 0.01