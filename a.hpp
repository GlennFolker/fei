#ifndef GUARD
#define GUARD

template<class T> struct my_stuff {
    static void do_stuff();
};

extern template struct my_stuff<int>;

#endif
