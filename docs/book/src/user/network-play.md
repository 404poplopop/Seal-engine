# Network play

Two Seal windows can play the same game together, on one computer or across a local network.

1. In the first window, choose **Host Network Play** from the Tools menu. Play starts, and a notice shows the port, 47710 unless you pick another.
2. In the second window, open the same project and choose **Join Network Play**. Without an address it joins `127.0.0.1:47710`; for another computer, give its address, such as `192.168.1.20:47710`.

The host runs the game. A joined window sends its keys and buttons to the host and shows the host's game a tenth of a second behind, so movement stays smooth. Stopping play in either window ends its part of the session.

Hosting on the local network may make your system firewall ask whether Seal can accept connections.
