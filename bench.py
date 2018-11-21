import socket
import time
import random
import optparse
import threading

parser = optparse.OptionParser()
parser.add_option('-c', action='store', help='secnode host',
                  default='localhost')
parser.add_option('-n', action='store', type=int, help='number of messages',
                  default=1000)
parser.add_option('-s', action='store', type=int, help='number of subscribed',
                  default=10)
opts, args = parser.parse_args()

benchmark, = args


def create_socket(tp=socket.SOCK_STREAM):
    s = socket.socket(socket.AF_INET, tp)
    s.connect((opts.c, 10767))
    return s


def connect(n):
    con = []
    for i in range(n):
        con.append(create_socket())
    return con


def recvall(s, length):
    res = ''
    start = time.time()
    while len(res) < length and time.time() - start < 10:
        res += s.recv(length)
    return res


def ask_only():
    cons = connect(opts.s)
    query = ''.join('read cryo:target\n' for _ in range(opts.n))
    # NOTE: this expects that the timestamps are all zero, so that we can expect
    # a reproducible reply.
    rep = ''.join('update cryo:target [0.0,{"t":0.0}]\n' for _ in range(opts.n))
    t1 = time.time()
    for con in cons:
        con.sendall(query)
    for con in cons:
        rr = recvall(con, len(rep))
        assert rr == rep, rr[:1000]
    return t1


fn = globals()[benchmark]
t1 = fn()
t2 = time.time()
print '%.4f sec' % (t2 - t1)
