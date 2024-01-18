import matplotlib.pyplot as plt
import pandas as pd
import sys

only_ms = sys.argv[1] == 'ms'

for i in sys.argv[2:]:
    df = pd.read_csv(i)
    if only_ms:
        df.time_elapsed = df.time_elapsed/1000000

        # drop everything that's less than 1

        df = df[df.time_elapsed > 1]

    print("statistics:")
    print(f"\tmean: {df.time_elapsed.mean()}")
    print(f"\tmedian: {df.time_elapsed.median()}")
    print(f"\tstddev: {df.time_elapsed.std()}")
    print(f"\tmin: {df.time_elapsed.min()}")
    print(f"\tmax: {df.time_elapsed.max()}")

    plt.plot(df.index, df.time_elapsed)
    plt.ylabel("time elapsed (in ms)")
    plt.xlabel("iteration of decoding")
    plt.show()
