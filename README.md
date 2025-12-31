# cuckoo

cuckoo is the combination of API client and API interceptor/proxy

API client is pretty popular, like postman, postwoman, yaak

API interceptor/proxy is also popular, like Fiddler, Charles, and Mitmproxy

Even the combination idea is not the first, there is already [reqable](https://reqable.com/zh-CN/) and requestly

But there is still some place to improve. This software trys to bring the best of them

The purpose is to be:
1. easy to use, and explain the details if you want to know
2. lightweight, 
3. local firstly
4. performance
5. open sourced and free
6. duration and safe

#2, use tauri, rather than electron. The size will be much smaller. And we tried not to add all features. esbuild is a good example.

#4, Mitmproxy is the target, it does have some great features, but not that 

#6, although it's not that important, the data is designed to be stored in sqlite, this makes the data safe when things goes wrong.



